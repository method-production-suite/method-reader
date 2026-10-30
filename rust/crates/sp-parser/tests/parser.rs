//! Port of tests/parser.test.ts.
//!
//! The classifier, the block grouper, title-page detection, scene-number
//! attachment, cue normalisation and the roster rescue, all on hand-built
//! blocks, lines and elements. No PDF is read, so nothing here needs the
//! oracle and nothing was retired or moved: every TypeScript test has its
//! `#[test]` here, one for one, grouped by `describe` block.
//!
//! Ids: the TypeScript leaned on the module counter (`resetCounter` in a
//! `beforeEach`). Rust tests run on parallel threads, so every test that
//! asserts an id brings its own counter ([`counter`]). The one exception is
//! `reset_counter_resets_id_sequence`, which exercises `reset_counter` and
//! the shared `default_next_id` themselves: it is the only user of that
//! global counter in this test binary (each integration-test file is its own
//! process), so no other test can race it.

mod common;

use sp_model::{ElementType, FamilyBucket, Fmt, RawLine, ScreenplayElement, SizeStep, TextBlock};
use sp_parser::classify::{
    attach_scene_numbers, classify_block, default_next_id, normalize_cue_name, reset_counter,
};
use sp_parser::group::group_blocks;
use sp_parser::rescue::rescue_cues;
use sp_parser::title_page::detect_title_pages;

// ── Helpers ──────────────────────────────────────────────

/// A call-scoped id source: `elem0`, `elem1`, ...
fn counter() -> impl FnMut() -> String {
    let mut n = 0u32;
    move || {
        n += 1;
        format!("elem{}", n - 1)
    }
}

/// `makeBlock({ text, indent })`: one line at y 100 on page 1.
fn make_block(text: &str, indent: f64) -> TextBlock {
    make_block_at(text, indent, 1, 100.0)
}

/// `makeBlock({ text, indent, pageNum, yPosition })`.
fn make_block_at(text: &str, indent: f64, page_num: u32, y_position: f64) -> TextBlock {
    let lines = vec![make_line(text, indent, y_position, page_num)];
    make_block_with_lines(text, indent, page_num, y_position, lines)
}

/// `makeBlock({ text, indent, lines })` and the general case.
fn make_block_with_lines(
    text: &str,
    indent: f64,
    page_num: u32,
    y_position: f64,
    lines: Vec<RawLine>,
) -> TextBlock {
    TextBlock {
        lines,
        text: text.to_owned(),
        styled_text: None,
        fmt: None,
        dual_right: None,
        indent,
        min_indent: indent,
        max_indent: indent,
        page_num,
        y_position,
    }
}

/// `makeLine({ text, indent, y, pageNum })`.
fn make_line(text: &str, indent: f64, y: f64, page_num: u32) -> RawLine {
    RawLine {
        text: text.to_owned(),
        indent,
        y,
        page_num,
        styled: None,
        dual_right: None,
        fonts: None,
        fmt: None,
        placed: false,
    }
}

/// `classifyBlock(block, prev)` where the id is not asserted.
fn classify(block: &TextBlock, prev: Option<&ScreenplayElement>) -> ScreenplayElement {
    classify_block(block, prev, &mut counter())
}

/// The `makeElement` of the title-page and scene-number blocks (and `el` of
/// the rescue block, with id `x`): the required fields, defaults elsewhere.
fn make_element(kind: ElementType, text: &str, page_num: u32) -> ScreenplayElement {
    ScreenplayElement {
        kind,
        text: text.to_owned(),
        id: "test".to_owned(),
        page_num,
        is_title_page: false,
        is_readable: true,
        character: None,
        base_character: None,
        age_modifier: None,
        age_value: None,
        scene_number: None,
        styled_text: None,
        dual_right: None,
        fmt: None,
        img: None,
        y: None,
    }
}

// ── classify.rs ──────────────────────────────────────────

mod classify_block {
    use super::*;

    mod scene_headings {
        use super::*;

        #[test]
        fn int_prefix_classified_as_scene() {
            let block = make_block("INT. KITCHEN - DAY", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Scene);
            assert_eq!(result.text, "INT. KITCHEN - DAY");
        }

        #[test]
        fn ext_prefix_classified_as_scene() {
            let block = make_block("EXT. PARK - NIGHT", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Scene);
        }

        #[test]
        fn int_ext_prefix_classified_as_scene() {
            let block = make_block("INT./EXT. CAR - MOVING - DAY", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Scene);
        }

        #[test]
        fn i_e_prefix_classified_as_scene() {
            let block = make_block("I/E. HOUSE - CONTINUOUS", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Scene);
        }

        #[test]
        fn scene_heading_at_any_indent_is_still_scene() {
            let block = make_block("INT. OFFICE - DAY", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Scene);
        }

        #[test]
        fn inline_dual_margin_scene_numbers_strip_and_attach_signal_lost() {
            // Some generators print the shooting-script number in both margins of
            // the heading row itself, joined into one line by extraction.
            let block = make_block("2 EXT. DESERT HIGHWAY - WIDE - ESTABLISHING 2", 9.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Scene);
            assert_eq!(result.text, "EXT. DESERT HIGHWAY - WIDE - ESTABLISHING");
            assert_eq!(result.scene_number.as_deref(), Some("2"));
        }

        #[test]
        fn inline_dual_margin_numbers_with_letter_suffix_attach_too() {
            let block = make_block("12A INT. HALLWAY - CONTINUOUS 12A", 9.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Scene);
            assert_eq!(result.scene_number.as_deref(), Some("12A"));
        }

        #[test]
        fn mismatched_margin_numbers_do_not_strip() {
            let block = make_block("2 EXT. WOODS - DAY 3", 9.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Action);
        }
    }

    mod character_names {
        use super::*;

        #[test]
        fn uppercase_name_at_character_indent() {
            let block = make_block("JACK", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("JACK"));
            assert_eq!(result.base_character.as_deref(), Some("JACK"));
        }

        #[test]
        fn character_with_v_o_extension() {
            let block = make_block("SARAH (V.O.)", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("SARAH"));
        }

        #[test]
        fn character_with_cont_d_extension() {
            let block = make_block("DEV (CONT'D)", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("DEV"));
        }

        #[test]
        fn character_with_o_s_extension() {
            let block = make_block("DETECTIVE JONES (O.S.)", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("DETECTIVE JONES"));
        }

        // Dotted names: a period in abbreviation position is part of the name,
        // not sentence punctuation. The old "periods only in ellipsis" guard
        // rejected every bare dotted cue ("MR. SMITH") while the extension
        // branch let "MR. SMITH (V.O.)" through — recognition depended on
        // incidental punctuation, the same asymmetry as the (O.S) bug below.
        #[test]
        fn honorific_with_a_period_is_a_character_name() {
            let block = make_block("MR. HENDERSON", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("MR. HENDERSON"));
        }

        #[test]
        fn dotted_initials_are_a_character_name() {
            let block = make_block("J.J.", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
        }

        #[test]
        fn trailing_single_letter_initial_is_a_character_name() {
            let block = make_block("ANNA B.", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
        }

        #[test]
        fn initials_plus_surname_are_a_character_name() {
            let block = make_block("E.B. WHITE", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
        }

        #[test]
        fn dotted_honorific_with_an_extension_still_works() {
            let block = make_block("DR. WHO (V.O.)", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("DR. WHO"));
        }

        // The guard's real job survives: a period closing a longer final word
        // is sentence punctuation — all-caps action prose that drifted into
        // the cue band must not become a phantom speaker (registry §9e).
        #[test]
        fn an_all_caps_sentence_at_cue_indent_is_not_a_character() {
            let block = make_block("HE STOPS.", 40.0);
            let result = classify(&block, None);
            assert_ne!(result.kind, ElementType::Character);
        }

        #[test]
        fn a_shouted_word_with_a_period_is_not_a_character() {
            let block = make_block("STOP.", 40.0);
            let result = classify(&block, None);
            assert_ne!(result.kind, ElementType::Character);
        }

        // Writers routinely drop the closing period: one script has the same
        // speaker as both "RADIO VOICE (O.S)" and "RADIO VOICE (O.S.)". The
        // unpunctuated form used to fall through the extension branch and then
        // hit the "periods only in ellipsis" guard, so the cue AND its speech
        // both landed as action.
        #[test]
        fn character_with_a_period_less_o_s_extension() {
            let block = make_block("RADIO VOICE (O.S)", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("RADIO VOICE"));
        }

        #[test]
        fn character_with_a_period_less_v_o_extension() {
            let block = make_block("SARAH (V.O)", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("SARAH"));
        }

        #[test]
        fn character_with_a_period_less_o_c_extension() {
            let block = make_block("DEV (O.C)", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("DEV"));
        }

        #[test]
        fn both_spellings_of_one_speaker_yield_the_same_character_name() {
            let dotted = classify(&make_block("RADIO VOICE (O.S.)", 40.0), None);
            let bare = classify(&make_block("RADIO VOICE (O.S)", 40.0), None);
            assert_eq!(bare.character, dotted.character);
            assert_eq!(bare.base_character, dotted.base_character);
        }

        // One script types MARGO’S MOM with a curly apostrophe. The name
        // pattern allowed only the straight ', so every bare instance fell to
        // action — while "MARGO’S MOM (O.S.)" passed, because an extension takes
        // a different branch that never reaches CHARACTER_NAME.
        #[test]
        fn character_name_with_a_curly_apostrophe() {
            let block = make_block("MARGO’S MOM", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("MARGO’S MOM"));
        }

        #[test]
        fn character_name_with_a_straight_apostrophe_still_works() {
            let block = make_block("MARGO'S DAD", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("MARGO'S DAD"));
        }

        #[test]
        fn rejects_character_name_at_action_indent() {
            let block = make_block("JACK", 10.0);
            let result = classify(&block, None);
            assert_ne!(result.kind, ElementType::Character);
        }

        #[test]
        fn rejects_lowercase_text_at_character_indent() {
            let block = make_block("Jack walks in", 40.0);
            let result = classify(&block, None);
            assert_ne!(result.kind, ElementType::Character);
        }

        #[test]
        fn rejects_names_with_punctuation() {
            let block = make_block("JACK!", 40.0);
            let result = classify(&block, None);
            assert_ne!(result.kind, ElementType::Character);
        }

        #[test]
        fn rejects_company_names() {
            let block = make_block("ACME INC", 40.0);
            let result = classify(&block, None);
            assert_ne!(result.kind, ElementType::Character);
        }
    }

    mod age_variants {
        use super::*;
        use sp_model::AgeValue;

        #[test]
        fn young_prefix_extracts_age_modifier() {
            let block = make_block("YOUNG JACK", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.base_character.as_deref(), Some("JACK"));
            assert_eq!(result.age_modifier.as_deref(), Some("YOUNG"));
            assert_eq!(result.age_value, Some(AgeValue::Text("young".to_owned())));
        }

        #[test]
        fn old_prefix_extracts_age_modifier() {
            let block = make_block("OLD JACK", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.base_character.as_deref(), Some("JACK"));
            assert_eq!(result.age_modifier.as_deref(), Some("OLD"));
        }

        #[test]
        fn little_prefix_extracts_child_age() {
            let block = make_block("LITTLE SARAH", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.base_character.as_deref(), Some("SARAH"));
            assert_eq!(result.age_modifier.as_deref(), Some("LITTLE"));
            assert_eq!(result.age_value, Some(AgeValue::Text("child".to_owned())));
        }
    }

    mod dialogue {
        use super::*;

        #[test]
        fn text_at_dialogue_indent_after_character_is_dialogue() {
            let char_block = make_block("JACK", 40.0);
            let char_element = classify(&char_block, None);

            let dial_block = make_block("Hello there.", 30.0);
            let result = classify(&dial_block, Some(&char_element));
            assert_eq!(result.kind, ElementType::Dialogue);
            assert_eq!(result.character.as_deref(), Some("JACK"));
        }

        #[test]
        fn dialogue_after_dialogue_preserves_character() {
            let char_block = make_block("SARAH", 40.0);
            let char_element = classify(&char_block, None);

            let dial1_block = make_block("First line.", 30.0);
            let dial1 = classify(&dial1_block, Some(&char_element));

            let dial2_block = make_block("Second line.", 30.0);
            let result = classify(&dial2_block, Some(&dial1));
            assert_eq!(result.kind, ElementType::Dialogue);
            assert_eq!(result.character.as_deref(), Some("SARAH"));
        }

        #[test]
        fn dialogue_after_action_has_no_character_defaults_to_action() {
            let action_block = make_block("Jack walks in.", 10.0);
            let action_element = classify(&action_block, None);

            let dial_block = make_block("Hello there.", 30.0);
            let result = classify(&dial_block, Some(&action_element));
            // No active character, so it defaults to action
            assert_eq!(result.kind, ElementType::Action);
        }
    }

    mod parentheticals {
        use super::*;

        #[test]
        fn parenthetical_after_character() {
            let char_block = make_block("JACK", 40.0);
            let char_element = classify(&char_block, None);

            let paren_block = make_block("(whispering)", 30.0);
            let result = classify(&paren_block, Some(&char_element));
            assert_eq!(result.kind, ElementType::Parenthetical);
            assert_eq!(result.character.as_deref(), Some("JACK"));
        }

        #[test]
        fn parenthetical_after_dialogue_preserves_character() {
            let char_block = make_block("SARAH", 40.0);
            let char_element = classify(&char_block, None);

            let dial_block = make_block("Wait...", 30.0);
            let dial = classify(&dial_block, Some(&char_element));

            let paren_block = make_block("(beat)", 30.0);
            let result = classify(&paren_block, Some(&dial));
            assert_eq!(result.kind, ElementType::Parenthetical);
            assert_eq!(result.character.as_deref(), Some("SARAH"));
        }

        #[test]
        fn parenthetical_without_active_character_is_not_parenthetical() {
            let paren_block = make_block("(whispering)", 30.0);
            let result = classify(&paren_block, None);
            assert_ne!(result.kind, ElementType::Parenthetical);
        }
    }

    mod action {
        use super::*;

        #[test]
        fn text_at_low_indent_classified_as_action() {
            let block = make_block("The door slams shut.", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Action);
        }

        #[test]
        fn possessive_start_detected_as_action() {
            let block = make_block("Jack's eyes widen.", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Action);
        }

        #[test]
        fn pronoun_start_detected_as_action() {
            let block = make_block("She turns around slowly.", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Action);
        }

        #[test]
        fn verb_pattern_detected_as_action() {
            let block = make_block("The stranger walks into the room.", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Action);
        }
    }

    mod transitions {
        use super::*;

        #[test]
        fn cut_to_at_high_indent_classified_as_transition() {
            let block = make_block("CUT TO:", 60.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Transition);
        }

        #[test]
        fn fade_to_classified_as_transition() {
            let block = make_block("FADE TO:", 60.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Transition);
        }

        #[test]
        fn transition_requires_high_indent() {
            let block = make_block("CUT TO:", 10.0);
            let result = classify(&block, None);
            // At low indent, matches action by camera pattern
            assert_ne!(result.kind, ElementType::Transition);
        }
    }

    mod page_numbers {
        use super::*;

        #[test]
        fn number_at_high_indent_classified_as_page_number() {
            let block = make_block("42", 70.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn number_with_period_at_high_indent() {
            let block = make_block("42.", 70.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn bare_number_at_low_indent_is_still_page_number() {
            let block = make_block("42", 10.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn bare_number_at_center_indent_is_page_number() {
            let block = make_block("42", 45.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn dashed_page_number_format_42() {
            let block = make_block("- 42 -", 45.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn page_42_format_is_page_number() {
            let block = make_block("Page 42", 45.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn p_42_format_is_page_number() {
            let block = make_block("p. 42", 45.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn number_with_surrounding_whitespace_is_page_number() {
            let block = make_block("  42  ", 70.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn revision_slug_classified_as_page_number() {
            let block = make_block_at("Blue Rev. (6/12/26) 15.15", 5.0, 15, 100.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::PageNumber);
        }

        #[test]
        fn color_named_character_cue_is_not_boilerplate() {
            let block = make_block("CHERRY", 40.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Character);
            assert_eq!(result.character.as_deref(), Some("CHERRY"));
        }
    }

    mod mini_slugs {
        use super::*;

        // Extraction measures indent from the PAGE edge, so the real action
        // margin is 15-18 across every generator — not the 0-4 the first
        // implementation assumed. These blocks sit where real ones do.
        fn slug(text: &str) -> ScreenplayElement {
            slug_at(text, 17.0)
        }

        fn slug_at(text: &str, indent: f64) -> ScreenplayElement {
            classify(&make_block(text, indent), None)
        }

        fn kind(text: &str, indent: f64) -> ElementType {
            classify(&make_block(text, indent), None).kind
        }

        #[test]
        fn short_uppercase_text_at_low_indent() {
            let block = make_block("LATER", 2.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::MiniSlug);
        }

        #[test]
        fn long_uppercase_text_at_low_indent() {
            let block = make_block("MOMENTS LATER", 2.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::MiniSlug);
        }

        #[test]
        fn scene_heading_prefix_not_treated_as_mini_slug() {
            let block = make_block("INT. OFFICE", 2.0);
            let result = classify(&block, None);
            assert_eq!(result.kind, ElementType::Scene);
        }

        // A mini-slug serializes as a forced slugline (".LATER"). fountain-js
        // promotes a forced line back to a FULL scene heading — section, TOC
        // entry and all — whenever the text also matches its unforced-heading
        // shape, which is wider than this file's SCENE_HEADING (EST., the space
        // forms, bare I/E). Minting a mini-slug from one of those would round-trip
        // into a scene. See PRIMARY_SLUG.
        #[test]
        fn an_est_slug_is_not_a_mini_slug_it_would_promote_on_re_parse() {
            assert_eq!(kind("EST. THE HOUSE", 2.0), ElementType::Action);
        }

        #[test]
        fn the_space_forms_and_bare_i_e_are_not_mini_slugs_either() {
            assert_eq!(kind("INT HOUSE - DAY", 2.0), ElementType::Action);
            assert_eq!(kind("EXT YARD - NIGHT", 2.0), ElementType::Action);
            assert_eq!(kind("I/E CAR - DAY", 2.0), ElementType::Action);
            assert_eq!(kind("INT/EXT CAR - DAY", 2.0), ElementType::Action);
        }

        #[test]
        fn a_word_that_merely_starts_with_those_letters_stays_a_mini_slug() {
            // The opener needs its dot or space: ESTABLISHING is not EST.
            assert_eq!(kind("ESTABLISHING SHOT", 2.0), ElementType::MiniSlug);
            assert_eq!(kind("INTO THE WOODS", 2.0), ElementType::MiniSlug);
        }

        // The same widened exclusion at the real action margin, and with an
        // unpaired shooting-script number pushing the opener off the anchor.
        #[test]
        fn the_widened_heading_exclusion_holds_at_the_action_margin_too() {
            assert_eq!(slug("EST. THE HOUSE").kind, ElementType::Action);
            assert_eq!(slug("I/E CAR - DAY").kind, ElementType::Action);
            assert_eq!(slug("ESTABLISHING SHOT").kind, ElementType::MiniSlug);
        }

        // ── accepted shapes, at the real action margin ────────
        #[test]
        fn a_bare_location_slug_at_the_action_margin() {
            assert_eq!(slug("IN THE PROJECTION BOOTH").kind, ElementType::MiniSlug);
        }

        #[test]
        fn a_one_word_position_slug() {
            assert_eq!(slug("UPSTAIRS").kind, ElementType::MiniSlug);
        }

        #[test]
        fn a_montage_bracket_bare() {
            assert_eq!(slug("END OF MONTAGE").kind, ElementType::MiniSlug);
        }

        #[test]
        fn a_colon_terminated_slug() {
            assert_eq!(slug("QUICK MONTAGE:").kind, ElementType::MiniSlug);
        }

        #[test]
        fn a_slug_with_an_internal_dash() {
            assert_eq!(slug("MONTAGE - VARIOUS").kind, ElementType::MiniSlug);
        }

        #[test]
        fn a_time_slug() {
            assert_eq!(slug("ANOTHER MOMENT:").kind, ElementType::MiniSlug);
        }

        // ── the danger set: sound effects and shouted beats ───
        #[test]
        fn a_period_terminated_sound_effect_stays_action() {
            assert_eq!(slug("THUD.").kind, ElementType::Action);
        }

        #[test]
        fn a_shouted_beat_stays_action() {
            assert_eq!(slug("CRASH!").kind, ElementType::Action);
        }

        #[test]
        fn a_repeated_sound_effect_stays_action() {
            assert_eq!(slug("BEEP. BEEP.").kind, ElementType::Action);
        }

        #[test]
        fn a_one_word_bang_stays_action() {
            assert_eq!(slug("DING!").kind, ElementType::Action);
        }

        #[test]
        fn an_all_caps_action_sentence_stays_action() {
            assert_eq!(
                slug("GLASSES CLINK ON THE FAR SIDE OF THE ROOM.").kind,
                ElementType::Action
            );
        }

        #[test]
        fn a_line_broken_off_mid_thought_with_a_dash_stays_action() {
            assert_eq!(
                slug("HANDS TREMBLE AS THE LATCH GIVES-").kind,
                ElementType::Action
            );
        }

        #[test]
        fn a_revision_starred_production_note_stays_action() {
            assert_eq!(
                slug("***SHOT ON THE SECOND CAMERA***").kind,
                ElementType::Action
            );
        }

        // ── transitions must not become headings ──────────────
        #[test]
        fn fountain_s_own_transition_rule_wins_anything_ending_in_to() {
            assert_eq!(slug("DISSOLVE TO:").kind, ElementType::Action);
            assert_eq!(slug("SMASH CUT TO:").kind, ElementType::Action);
        }

        #[test]
        fn a_camera_prefixed_line_is_still_vetoed_at_priority_7() {
            assert_eq!(slug("FADE IN:").kind, ElementType::Action);
        }

        // ── the priority-7 decision, recorded as behavior ─────
        #[test]
        fn a_the_prefixed_slug_stays_action_priority_7_keeps_its_veto() {
            // Deliberate recall loss. "THE DOOR OPENS." and "THE KITCHEN" are
            // the same shape one period apart, so the pronoun veto stays.
            assert_eq!(slug("THE PROJECTION BOOTH").kind, ElementType::Action);
            assert_eq!(slug("THE DOOR OPENS").kind, ElementType::Action);
        }

        // ── shape guards ──────────────────────────────────────
        #[test]
        fn a_wrapped_multi_line_block_is_never_a_slug() {
            // Deliberately opens on a NON-pronoun word: "THE ..." would be vetoed
            // at priority 7 and the test would pass without the single-line guard.
            let block = make_block_with_lines(
                "LANTERNS SWING OVER THE COUNTER AND NOBODY SPEAKS",
                17.0,
                1,
                100.0,
                vec![
                    make_line("LANTERNS SWING OVER THE", 17.0, 100.0, 1),
                    make_line("COUNTER AND NOBODY SPEAKS", 17.0, 88.0, 1),
                ],
            );
            assert_eq!(classify(&block, None).kind, ElementType::Action);
            // Guard against the test passing for the wrong reason: the same text on
            // ONE line is a slug, so only the line count can be deciding it.
            assert_eq!(
                slug("LANTERNS SWING OVER THE COUNTER AND NOBODY SPEAKS").kind,
                ElementType::MiniSlug
            );
        }

        #[test]
        fn the_action_band_ceiling_holds_a_right_flush_line_is_never_a_slug() {
            // Nothing else stops this one. TRANSITION needs a trailing colon,
            // the cue band ends at 50, and priority 7 only runs below ACTION_MAX.
            assert_eq!(slug_at("SMASH TO BLACK", 60.0).kind, ElementType::Action);
            assert_eq!(slug_at("SMASH TO BLACK", 17.0).kind, ElementType::MiniSlug);
        }

        #[test]
        fn a_bare_parenthetical_with_no_active_character_is_never_a_slug() {
            // Priority 6 needs a speaker, so this falls all the way to 9.
            assert_eq!(slug("(BEAT)").kind, ElementType::Action);
            assert_eq!(slug("(SILENCE)").kind, ElementType::Action);
        }

        #[test]
        fn mixed_case_is_never_a_slug() {
            assert_eq!(slug("In the projection booth").kind, ElementType::Action);
            assert_eq!(slug("Dev CROSSES TO THE COUNTER").kind, ElementType::Action);
        }

        #[test]
        fn a_digit_dominated_registration_line_is_never_a_slug() {
            assert_eq!(slug("WGA 1234567 555-0100").kind, ElementType::Action);
        }

        #[test]
        fn an_unpaired_shooting_script_heading_is_never_a_slug() {
            // The dual-margin strip needs matching numbers; when they mismatch
            // the heading survives whole and must not read as a micro-heading.
            assert_eq!(
                slug_at("2 EXT. WOODS - DAY 3", 9.0).kind,
                ElementType::Action
            );
        }

        #[test]
        fn an_over_long_line_is_never_a_slug() {
            assert_eq!(
                slug("MONTAGE - EVERY DOORWAY IN THE BUILDING, ONE AFTER ANOTHER").kind,
                ElementType::Action
            );
        }

        // ── band non-interference ─────────────────────────────
        #[test]
        fn the_rule_never_fires_in_the_cue_band() {
            // 35-50 is the cue band; a bare uppercase word there is a character.
            assert_eq!(kind("LATER", 40.0), ElementType::Character);
            assert_eq!(kind("UPSTAIRS", 43.0), ElementType::Character);
        }

        #[test]
        fn the_rule_never_fires_in_the_speech_band() {
            let cue = classify(&make_block("MARGO", 43.0), None);
            let speech = classify(&make_block("UPSTAIRS", 29.0), Some(&cue));
            assert_eq!(speech.kind, ElementType::Dialogue);
        }
    }

    mod element_ids {
        use super::*;

        #[test]
        fn sequential_ids_are_assigned() {
            let mut next = counter();
            let block1 = make_block("INT. KITCHEN - DAY", 10.0);
            let block2 = make_block("JACK", 40.0);
            let el1 = classify_block(&block1, None, &mut next);
            let el2 = classify_block(&block2, Some(&el1), &mut next);
            assert_eq!(el1.id, "elem0");
            assert_eq!(el2.id, "elem1");
        }

        // The one test that drives the shared counter itself: see the file's
        // doc comment for why nothing else in this binary can race it.
        #[test]
        fn reset_counter_resets_id_sequence() {
            reset_counter();
            let block = make_block("INT. OFFICE - DAY", 10.0);
            classify_block(&block, None, &mut default_next_id);
            reset_counter();
            let result = classify_block(&block, None, &mut default_next_id);
            assert_eq!(result.id, "elem0");
        }
    }
}

// ── group.rs ─────────────────────────────────────────────

mod group_blocks {
    use super::*;

    #[test]
    fn contiguous_lines_with_same_indent_form_one_block() {
        let lines = vec![
            make_line("First line of action.", 10.0, 700.0, 1),
            make_line("Second line of action.", 10.0, 688.0, 1),
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].text,
            "First line of action. Second line of action."
        );
    }

    #[test]
    fn scene_heading_starts_new_block() {
        let lines = vec![
            make_line("Some action text.", 10.0, 700.0, 1),
            make_line("INT. KITCHEN - DAY", 10.0, 688.0, 1),
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "Some action text.");
        assert_eq!(blocks[1].text, "INT. KITCHEN - DAY");
    }

    #[test]
    fn large_y_gap_breaks_block() {
        let lines = vec![
            make_line("First block.", 10.0, 700.0, 1),
            make_line("Second block.", 10.0, 650.0, 1),
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn page_break_starts_new_block() {
        let lines = vec![
            make_line("Page one text.", 10.0, 100.0, 1),
            make_line("Page two text.", 10.0, 700.0, 2),
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn indent_change_breaks_block() {
        let lines = vec![
            make_line("Action text.", 10.0, 700.0, 1),
            make_line("Dialogue text.", 30.0, 688.0, 1),
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn character_indent_starts_new_block() {
        let lines = vec![
            make_line("Some dialogue.", 30.0, 700.0, 1),
            make_line("JACK", 40.0, 688.0, 1),
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn cleans_ellipsis_artifacts_from_character_names() {
        let lines = vec![make_line("JACK...", 40.0, 700.0, 1)];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks[0].text, "JACK");
    }

    #[test]
    fn block_inherits_page_num_and_y_position_from_first_line() {
        let lines = vec![
            make_line("Line one.", 10.0, 500.0, 3),
            make_line("Line two.", 10.0, 488.0, 3),
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks[0].page_num, 3);
        assert_eq!(blocks[0].y_position, 500.0);
    }

    #[test]
    fn block_tracks_min_and_max_indent() {
        let lines = vec![
            make_line("Line one.", 10.0, 700.0, 1),
            make_line("Line two.", 11.0, 688.0, 1),
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks[0].min_indent, 10.0);
        assert_eq!(blocks[0].max_indent, 11.0);
    }
}

// ── title_page.rs ────────────────────────────────────────

mod detect_title_pages {
    use super::*;

    fn action(text: &str, page_num: u32) -> ScreenplayElement {
        make_element(ElementType::Action, text, page_num)
    }

    fn scene(text: &str, page_num: u32) -> ScreenplayElement {
        make_element(ElementType::Scene, text, page_num)
    }

    #[test]
    fn elements_before_first_scene_on_page_1_are_marked_as_title_page() {
        let elements = vec![
            action("MY SCREENPLAY", 1),
            action("Written by", 1),
            action("John Doe", 1),
            scene("INT. KITCHEN - DAY", 2),
        ];
        let result = detect_title_pages(&elements);
        assert!(result[0].is_title_page);
        assert!(result[1].is_title_page);
        assert!(result[2].is_title_page);
        assert!(!result[3].is_title_page);
    }

    #[test]
    fn title_and_by_line_are_readable() {
        let elements = vec![
            action("MY SCREENPLAY", 1),
            action("Written by", 1),
            scene("INT. KITCHEN - DAY", 2),
        ];
        let result = detect_title_pages(&elements);
        assert!(result[0].is_readable);
        assert!(result[1].is_readable);
    }

    #[test]
    fn contact_info_after_by_line_is_not_readable() {
        let elements = vec![
            action("MY SCREENPLAY", 1),
            action("Written by", 1),
            action("John Doe", 1),
            action("john@email.com", 1),
            action("Phone: (555) 123-4567", 1),
            action("Draft 2", 1),
            scene("INT. KITCHEN - DAY", 2),
        ];
        let result = detect_title_pages(&elements);
        assert!(result[2].is_readable); // Author name
        assert!(!result[3].is_readable); // Email
        assert!(!result[4].is_readable); // Phone
        assert!(!result[5].is_readable); // Draft
    }

    #[test]
    fn front_matter_on_pages_2_is_readable() {
        let elements = vec![
            action("MY SCREENPLAY", 1),
            action("Written by Author", 1),
            action("For my mother.", 2),
            scene("INT. KITCHEN - DAY", 3),
        ];
        let result = detect_title_pages(&elements);
        assert!(!result[2].is_title_page);
        assert!(result[2].is_readable);
    }

    #[test]
    fn no_changes_when_first_element_is_scene() {
        let elements = vec![
            scene("INT. KITCHEN - DAY", 1),
            make_element(ElementType::Character, "JACK", 1),
        ];
        let result = detect_title_pages(&elements);
        assert!(!result[0].is_title_page);
        assert!(!result[1].is_title_page);
    }

    #[test]
    fn returns_elements_unchanged_when_no_scene_found() {
        let elements = vec![action("Just some text", 1)];
        let result = detect_title_pages(&elements);
        assert!(!result[0].is_title_page);
    }

    #[test]
    fn copyright_is_detected_as_contact_info() {
        let elements = vec![
            action("MY SCREENPLAY", 1),
            action("by Author", 1),
            action("Copyright 2024", 1),
            scene("INT. KITCHEN - DAY", 2),
        ];
        let result = detect_title_pages(&elements);
        assert!(!result[2].is_readable);
    }

    #[test]
    fn wga_registration_is_detected_as_contact_info() {
        let elements = vec![
            action("MY SCREENPLAY", 1),
            action("Screenplay by Author", 1),
            action("WGA Registration #12345", 1),
            scene("INT. KITCHEN - DAY", 2),
        ];
        let result = detect_title_pages(&elements);
        assert!(!result[2].is_readable);
    }
}

// ── index.ts: attachSceneNumbers ─────────────────────────

mod attach_scene_numbers {
    use super::*;

    // classifyBlock marks a shooting-script number by setting `sceneNumber` on
    // the page-number element itself; attachSceneNumbers only promotes what was
    // marked. It deliberately does NOT re-test the text, because by this point a
    // bare page number ("1.") and a bare scene number are the same type carrying
    // the same shape of text. See the pipeline tests for the end-to-end path
    // through the real classifier.
    #[test]
    fn shooting_script_scene_number_before_scene_heading_is_attached() {
        let mut marked = make_element(ElementType::PageNumber, "2.2.", 1);
        marked.scene_number = Some("2.2.".to_owned());
        let mut elements = vec![
            marked,
            make_element(ElementType::Scene, "INT. KITCHEN - DAY", 1),
        ];
        attach_scene_numbers(&mut elements);
        assert_eq!(elements[1].scene_number.as_deref(), Some("2.2."));
    }

    #[test]
    fn an_unmarked_page_number_before_a_scene_heading_is_not_attached() {
        let mut elements = vec![
            make_element(ElementType::PageNumber, "2.", 1),
            make_element(ElementType::Scene, "INT. KITCHEN - DAY", 1),
        ];
        attach_scene_numbers(&mut elements);
        assert_eq!(elements[1].scene_number, None);
    }

    #[test]
    fn revision_slug_before_scene_heading_is_not_attached_as_scene_number() {
        let mut elements = vec![
            make_element(ElementType::PageNumber, "Blue Rev. (6/12/26)", 1),
            make_element(ElementType::Scene, "INT. KITCHEN - DAY", 1),
        ];
        attach_scene_numbers(&mut elements);
        assert_eq!(elements[1].scene_number, None);
    }
}

// ── Integration: classify pipeline ───────────────────────

mod classify_pipeline_integration {
    use super::*;

    /// The TypeScript loop: each block classified against the one before.
    fn run(blocks: &[TextBlock]) -> Vec<ScreenplayElement> {
        let mut next = counter();
        let mut elements: Vec<ScreenplayElement> = Vec::new();
        for block in blocks {
            let el = classify_block(block, elements.last(), &mut next);
            elements.push(el);
        }
        elements
    }

    #[test]
    fn full_sequence_scene_character_dialogue_action() {
        let elements = run(&[
            make_block("INT. KITCHEN - DAY", 10.0),
            make_block("JACK", 40.0),
            make_block("What are we having for dinner?", 30.0),
            make_block("Sarah enters from the hallway.", 10.0),
        ]);

        assert_eq!(elements[0].kind, ElementType::Scene);
        assert_eq!(elements[1].kind, ElementType::Character);
        assert_eq!(elements[2].kind, ElementType::Dialogue);
        assert_eq!(elements[3].kind, ElementType::Action);
    }

    #[test]
    fn character_parenthetical_dialogue_sequence() {
        let elements = run(&[
            make_block("SARAH", 40.0),
            make_block("(whispering)", 30.0),
            make_block("Did you hear that?", 30.0),
        ]);

        assert_eq!(elements[0].kind, ElementType::Character);
        assert_eq!(elements[1].kind, ElementType::Parenthetical);
        assert_eq!(elements[1].character.as_deref(), Some("SARAH"));
        assert_eq!(elements[2].kind, ElementType::Dialogue);
        assert_eq!(elements[2].character.as_deref(), Some("SARAH"));
    }

    #[test]
    fn action_resets_character_context() {
        let elements = run(&[
            make_block("JACK", 40.0),
            make_block("Hello.", 30.0),
            make_block("The door opens.", 10.0),
            make_block("More text here.", 30.0),
        ]);

        assert_eq!(elements[0].kind, ElementType::Character);
        assert_eq!(elements[1].kind, ElementType::Dialogue);
        assert_eq!(elements[1].character.as_deref(), Some("JACK"));
        assert_eq!(elements[2].kind, ElementType::Action);
        // After action, no active character
        assert_eq!(elements[3].kind, ElementType::Action); // not dialogue, since no character
    }

    #[test]
    fn scene_heading_resets_character_context() {
        let elements = run(&[
            make_block("JACK", 40.0),
            make_block("Hello.", 30.0),
            make_block("INT. BEDROOM - NIGHT", 10.0),
            make_block("Some text.", 30.0),
        ]);

        assert_eq!(elements[2].kind, ElementType::Scene);
        // After scene, no active character
        assert_eq!(elements[3].kind, ElementType::Action);
    }

    #[test]
    fn multiple_character_switches() {
        let elements = run(&[
            make_block("JACK", 40.0),
            make_block("I don't think so.", 30.0),
            make_block("SARAH", 40.0),
            make_block("Why not?", 30.0),
        ]);

        assert_eq!(elements[1].character.as_deref(), Some("JACK"));
        assert_eq!(elements[3].character.as_deref(), Some("SARAH"));
    }
}

mod hybrid_and_non_standard_character_cues {
    use super::*;

    #[test]
    fn shared_cue_with_slash_is_a_character() {
        let block = make_block("MARGO/DEV", 40.0);
        let result = classify(&block, None);
        assert_eq!(result.kind, ElementType::Character);
        assert_eq!(result.character.as_deref(), Some("MARGO/DEV"));
    }

    #[test]
    fn numbered_cue_is_a_character() {
        let block = make_block("COP #2", 40.0);
        let result = classify(&block, None);
        assert_eq!(result.kind, ElementType::Character);
    }

    #[test]
    fn ampersand_cue_is_a_character() {
        let block = make_block("MOM & DAD", 40.0);
        let result = classify(&block, None);
        assert_eq!(result.kind, ElementType::Character);
    }

    #[test]
    fn shared_cue_carries_dialogue_context() {
        let cue = classify(&make_block("MARGO/DEV", 40.0), None);
        let paren = classify(&make_block("(singing along)", 30.0), Some(&cue));
        assert_eq!(paren.kind, ElementType::Parenthetical);
        let line = classify(&make_block("I never wanted that", 30.0), Some(&paren));
        assert_eq!(line.kind, ElementType::Dialogue);
        assert_eq!(line.character.as_deref(), Some("MARGO/DEV"));
    }
}

mod normalize_cue_name {
    use super::*;

    #[test]
    fn straightens_curly_apostrophes() {
        assert_eq!(normalize_cue_name("MARGO’S MOM"), "MARGO'S MOM");
    }

    #[test]
    fn upper_cases() {
        assert_eq!(normalize_cue_name("dev"), "DEV");
    }

    #[test]
    fn strips_a_trailing_extension() {
        assert_eq!(normalize_cue_name("MARGO’S MOM (O.S.)"), "MARGO'S MOM");
        assert_eq!(normalize_cue_name("DEV (CONT'D)"), "DEV");
    }

    #[test]
    fn collapses_internal_whitespace_and_trims() {
        assert_eq!(normalize_cue_name("  RADIO   VOICE  "), "RADIO VOICE");
    }

    #[test]
    fn leaves_an_ordinary_name_alone() {
        assert_eq!(normalize_cue_name("KARINA"), "KARINA");
    }
}

mod rescue_cues {
    use super::*;

    // elements[i] pairs with blocks[i]; only indent is read off the block.
    fn el(kind: ElementType, text: &str) -> ScreenplayElement {
        let mut e = make_element(kind, text, 1);
        e.id = "x".to_owned();
        e
    }

    /// A cue as classify leaves it: character and base name both set.
    fn cue(text: &str, name: &str) -> ScreenplayElement {
        let mut e = el(ElementType::Character, text);
        e.character = Some(name.to_owned());
        e.base_character = Some(name.to_owned());
        e
    }

    fn speech(text: &str, name: &str) -> ScreenplayElement {
        let mut e = el(ElementType::Dialogue, text);
        e.character = Some(name.to_owned());
        e
    }

    fn action(text: &str) -> ScreenplayElement {
        el(ElementType::Action, text)
    }

    fn blk(indent: f64) -> TextBlock {
        make_block("", indent)
    }

    #[test]
    fn promotes_a_lowercase_cue_when_the_name_is_in_the_roster() {
        let mut elements = vec![
            cue("DEV", "DEV"),
            speech("Hello.", "DEV"),
            action("dev"),
            action("Oh, right. The back room again."),
        ];
        rescue_cues(&mut elements, &[blk(40.0), blk(30.0), blk(40.0), blk(30.0)]);
        assert_eq!(elements[2].kind, ElementType::Character);
        assert_eq!(elements[2].base_character.as_deref(), Some("DEV"));
        assert_eq!(elements[3].kind, ElementType::Dialogue);
        assert_eq!(elements[3].character.as_deref(), Some("DEV"));
    }

    #[test]
    fn promotes_a_curly_apostrophe_cue_seen_once_with_an_extension() {
        let mut elements = vec![
            cue("MARGO’S MOM (O.S.)", "MARGO’S MOM"),
            speech("Hi.", "MARGO’S MOM"),
            action("MARGO’S MOM"),
            action("Put the rewinder back where it lives."),
        ];
        rescue_cues(&mut elements, &[blk(40.0), blk(30.0), blk(40.0), blk(30.0)]);
        assert_eq!(elements[2].kind, ElementType::Character);
        assert_eq!(elements[3].kind, ElementType::Dialogue);
    }

    #[test]
    fn does_not_promote_a_name_absent_from_the_roster() {
        let mut elements = vec![
            cue("DEV", "DEV"),
            speech("Hi.", "DEV"),
            action("STRANGER"),
            action("Some line."),
        ];
        rescue_cues(&mut elements, &[blk(40.0), blk(30.0), blk(40.0), blk(30.0)]);
        assert_eq!(elements[2].kind, ElementType::Action);
    }

    #[test]
    fn does_not_promote_outside_the_character_indent_band() {
        let mut elements = vec![
            cue("DEV", "DEV"),
            speech("Hi.", "DEV"),
            action("DEV"),
            action("Some line."),
        ];
        rescue_cues(&mut elements, &[blk(40.0), blk(30.0), blk(10.0), blk(30.0)]);
        assert_eq!(elements[2].kind, ElementType::Action);
    }

    #[test]
    fn does_not_promote_when_the_next_line_is_not_dialogue_indent() {
        let mut elements = vec![
            cue("DEV", "DEV"),
            speech("Hi.", "DEV"),
            action("DEV"),
            action("Some action at action indent."),
        ];
        rescue_cues(&mut elements, &[blk(40.0), blk(30.0), blk(40.0), blk(10.0)]);
        assert_eq!(elements[2].kind, ElementType::Action);
    }

    #[test]
    fn retypes_the_whole_speech_run_and_stops_at_the_boundary() {
        let mut elements = vec![
            cue("DEV", "DEV"),
            speech("Hi.", "DEV"),
            action("dev"),
            action("Line one."),
            action("Line two."),
            action("She leaves the room."),
        ];
        rescue_cues(
            &mut elements,
            &[
                blk(40.0),
                blk(30.0),
                blk(40.0),
                blk(30.0),
                blk(30.0),
                blk(10.0),
            ],
        );
        assert_eq!(elements[3].kind, ElementType::Dialogue);
        assert_eq!(elements[4].kind, ElementType::Dialogue);
        assert_eq!(elements[5].kind, ElementType::Action);
    }

    #[test]
    fn does_not_swallow_a_following_missed_cue_that_sits_in_the_overlap() {
        let mut elements = vec![
            cue("DEV", "DEV"),
            speech("Hi.", "DEV"),
            cue("MARGO", "MARGO"),
            speech("Hey.", "MARGO"),
            action("dev"),
            action("Line for Dev."),
            action("MARGO"), // second missed cue, indent 35
            action("Line for Margo."),
        ];
        rescue_cues(
            &mut elements,
            &[
                blk(40.0),
                blk(30.0),
                blk(40.0),
                blk(30.0),
                blk(40.0),
                blk(30.0),
                blk(35.0),
                blk(30.0),
            ],
        );
        assert_eq!(elements[4].kind, ElementType::Character);
        assert_eq!(elements[5].kind, ElementType::Dialogue);
        assert_eq!(elements[5].character.as_deref(), Some("DEV"));
        // The second cue must NOT have been eaten as Dev's dialogue.
        assert_eq!(elements[6].kind, ElementType::Character);
        assert_eq!(elements[6].character.as_deref(), Some("MARGO"));
        assert_eq!(elements[7].character.as_deref(), Some("MARGO"));
    }

    #[test]
    fn no_roster_means_no_rescues_and_no_crash() {
        let mut elements = vec![action("DEV"), action("Some line.")];
        rescue_cues(&mut elements, &[blk(40.0), blk(30.0)]);
        assert_eq!(elements[0].kind, ElementType::Action);
    }
}

mod font_shifts_and_block_boundaries {
    use super::*;

    fn ln(text: &str, y: f64, fmt: Option<Fmt>) -> RawLine {
        let mut line = make_line(text, 10.0, y, 1);
        line.fmt = fmt;
        line
    }

    fn sans() -> Option<Fmt> {
        Some(Fmt {
            family: Some(FamilyBucket::Sans),
            size: None,
        })
    }

    #[test]
    fn a_block_takes_the_fmt_every_one_of_its_lines_agrees_on() {
        let blocks = group_blocks(&[
            ln("A chyron line", 700.0, sans()),
            ln("and its second line", 688.0, sans()),
        ]);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].fmt, sans());
    }

    #[test]
    fn lines_that_disagree_leave_the_block_unmarked() {
        let blocks = group_blocks(&[
            ln("A chyron line", 700.0, sans()),
            ln("and a plain one", 688.0, None),
        ]);
        // They also do not merge: the fmt change is a block break.
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].fmt, sans());
        assert_eq!(blocks[1].fmt, None);
    }

    #[test]
    fn an_fmt_change_breaks_a_block_that_would_otherwise_merge() {
        // Same indent, same page, 12pt apart: without the fmt these are one block.
        let blocks = group_blocks(&[
            ln("plain action", 700.0, None),
            ln("SHOUTED IN ANOTHER FACE", 688.0, sans()),
            ln("plain again", 676.0, None),
        ]);
        let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(
            texts,
            ["plain action", "SHOUTED IN ANOTHER FACE", "plain again"]
        );
    }

    #[test]
    fn a_size_change_breaks_a_block_just_like_a_family_change() {
        let larger = Some(Fmt {
            family: None,
            size: Some(SizeStep::Larger),
        });
        let blocks = group_blocks(&[
            ln("plain action", 700.0, None),
            ln("INSERT: THE CARD", 688.0, larger),
        ]);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].fmt, larger);
    }

    #[test]
    fn lines_with_the_same_fmt_still_merge_normally() {
        let both = Some(Fmt {
            family: Some(FamilyBucket::Sans),
            size: Some(SizeStep::Larger),
        });
        let blocks = group_blocks(&[ln("one", 700.0, both), ln("two", 688.0, both)]);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn fmt_rides_from_block_to_element_without_touching_classification() {
        // fmt is never a classification input: the parser stays option-free.
        let block = group_blocks(&[ln("CHYRON: LIVE.", 700.0, sans())])
            .into_iter()
            .next()
            .expect("one block");
        let el = classify_block(&block, None, &mut counter());
        assert_eq!(el.kind, ElementType::Action);
        assert_eq!(el.fmt, sans());
    }
}
