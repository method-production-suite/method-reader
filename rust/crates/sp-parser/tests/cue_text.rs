//! Port of tests/cue-text.test.ts.
//!
//! The TS file builds one `accepts`/`rejects` test per row in a loop; here a
//! macro writes the same tests out, one `#[test]` per row, so a failing row
//! still names itself.

use sp_model::indent_ranges;
use sp_parser::classify::is_likely_character_name;
use sp_parser::cue::is_cue_text;

// ── ONE definition of what a character cue looks like ────────────────
//
// There used to be three, and they disagreed. `isLikelyCharacterName` in
// classify.ts carried forty lines of accumulated rules; `isCueShaped` in
// extract.ts, used only by dual-dialogue detection, carried three; and
// rescueCues reasons from the roster instead. The first two disagreeing is
// not hypothetical — it shipped. A script whose lead is named "Q" rendered
// every ordinary Q cue correctly, because the classifier accepts a
// one-letter name, while the dual detector rejected it and collapsed a
// whole scene into interleaved action.
//
// This is the same doctrine CLAUDE.md already applies to slug.ts and
// notes.ts: one copy of a discriminator both callers import, because two
// copies drift. The drift here cost a scene.
//
// What legitimately differs between callers is GEOMETRY, not shape. The
// classifier checks the indent band itself; the dual detector has already
// had its geometry settled upstream by clusterSplit, which demands a left
// cluster inside 42% of the page and a right one past 48%. So the shared
// function answers about TEXT only and each caller keeps its own position
// test.
mod is_cue_text_the_single_definition {
    use super::{indent_ranges, is_cue_text, is_likely_character_name};

    const IN_BAND: f64 = indent_ranges::CHARACTER_MIN + 1.0;

    const CUES: &[&str] = &[
        "JACK",
        "Q", // the one that cost us a scene
        "MR. WILTON",
        "ELDERLY MAN #3",
        "JACK (CONT'D)",
        "ALANI",
        "J.J.",
        "ANNA B.",
        "KATE EX WIFE",
    ];

    const NOT_CUES: &[&str] = &[
        "STOP.", // a shout in the dialogue band, registry 9e
        "What are you saying?",
        "He gazes around, rubs the fabric between his fingers.",
        "ACME PRODUCTIONS LLC",
        "Hey!",
        "",
    ];

    macro_rules! cue_case {
        ($name:ident, $text:expr, $want:expr) => {
            #[test]
            fn $name() {
                assert_eq!(is_cue_text($text), $want, "{:?}", $text);
            }
        };
    }

    cue_case!(accepts_jack, "JACK", true);
    cue_case!(accepts_q, "Q", true);
    cue_case!(accepts_mr_wilton, "MR. WILTON", true);
    cue_case!(accepts_elderly_man_3, "ELDERLY MAN #3", true);
    cue_case!(accepts_jack_cont_d, "JACK (CONT'D)", true);
    cue_case!(accepts_alani, "ALANI", true);
    cue_case!(accepts_j_j, "J.J.", true);
    cue_case!(accepts_anna_b, "ANNA B.", true);
    cue_case!(accepts_kate_ex_wife, "KATE EX WIFE", true);

    cue_case!(rejects_stop, "STOP.", false);
    cue_case!(rejects_what_are_you_saying, "What are you saying?", false);
    cue_case!(
        rejects_he_gazes_around_rubs_the_fabric_between_his_fingers,
        "He gazes around, rubs the fabric between his fingers.",
        false
    );
    cue_case!(rejects_acme_productions_llc, "ACME PRODUCTIONS LLC", false);
    cue_case!(rejects_hey, "Hey!", false);
    cue_case!(rejects_empty, "", false);

    // The invariant that matters more than any single row above: the
    // classifier must not grow a private opinion again. Given an indent
    // inside the cue band, it has to answer exactly what the shared function
    // answers, for every string either of them will ever see.
    #[test]
    fn the_classifier_agrees_with_the_shared_definition_in_band() {
        for t in CUES.iter().chain(NOT_CUES) {
            assert_eq!(
                is_likely_character_name(t, IN_BAND),
                is_cue_text(t),
                "{t:?}"
            );
        }
    }

    #[test]
    fn geometry_stays_with_the_caller_out_of_band_is_never_a_cue() {
        // isCueText says nothing about position, on purpose. The classifier is
        // what refuses a perfectly cue-shaped name sitting in the action margin.
        assert!(is_cue_text("JACK"));
        assert!(!is_likely_character_name(
            "JACK",
            indent_ranges::CHARACTER_MIN - 5.0
        ));
    }
}
