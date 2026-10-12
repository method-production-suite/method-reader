//! Port of the `resolveFormatOptions` and `keepSpeechesWhole option` blocks
//! of tests/options.test.ts, and the defaults pin against the repo's root
//! format-defaults.json. The file's other blocks test the CSS, the HTML
//! body, the EPUB and the serializer, and are ported by those crates
//! (rust/PARITY.md).

use serde_json::{Value, json};
use sp_model::{
    ContdMode, CueAlignment, DEFAULT_FORMAT_OPTIONS, DualDialogue, FontFamily, FormatOptions,
    resolve_format_options,
};

fn resolve(partial: Value) -> FormatOptions {
    resolve_format_options(Some(&partial), &DEFAULT_FORMAT_OPTIONS)
}

fn resolve_over(partial: Value, base: &FormatOptions) -> FormatOptions {
    resolve_format_options(Some(&partial), base)
}

fn canonical() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../format-defaults.json");
    let text = std::fs::read_to_string(path).expect("format-defaults.json at the repo root");
    serde_json::from_str(&text).expect("format-defaults.json is JSON")
}

#[test]
fn empty_input_yields_defaults() {
    assert_eq!(resolve(json!({})), DEFAULT_FORMAT_OPTIONS);
    assert_eq!(
        resolve_format_options(None, &DEFAULT_FORMAT_OPTIONS),
        DEFAULT_FORMAT_OPTIONS
    );
    assert_eq!(FormatOptions::default(), DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn defaults_match_the_canonical_format_defaults_json() {
    // Both directions: the file decodes into exactly the defaults, and the
    // defaults written out are exactly the file (same keys, and whole
    // numbers spelled as JavaScript spells them).
    let file = canonical();
    let decoded: FormatOptions = serde_json::from_value(file.clone()).unwrap();
    assert_eq!(decoded, DEFAULT_FORMAT_OPTIONS);
    assert_eq!(serde_json::to_value(DEFAULT_FORMAT_OPTIONS).unwrap(), file);
    assert_eq!(file["includeImages"], json!(true));
    assert_eq!(file["promoteCoverImage"], json!(true));
}

#[test]
fn defaults_serialize_in_the_file_s_key_order() {
    let file = canonical();
    let ours = serde_json::to_value(DEFAULT_FORMAT_OPTIONS).unwrap();
    let a: Vec<&String> = file.as_object().unwrap().keys().collect();
    let b: Vec<&String> = ours.as_object().unwrap().keys().collect();
    assert_eq!(a, b);
}

#[test]
fn numeric_knobs_are_clamped_to_sane_ranges() {
    let opts =
        resolve(json!({ "dialogueSideMarginPct": 90, "cueIndentPct": -5, "elementSpacingEm": 99 }));
    assert!(opts.dialogue_side_margin_pct <= 30.0);
    assert!(opts.cue_indent_pct >= 0.0);
    assert!(opts.element_spacing_em <= 2.0);
    // and exactly at the bounds, as Math.min/Math.max put them
    assert_eq!(opts.dialogue_side_margin_pct, 30.0);
    assert_eq!(opts.cue_indent_pct, 0.0);
    assert_eq!(opts.element_spacing_em, 2.0);
    assert_eq!(
        resolve(json!({ "elementSpacingEm": 0 })).element_spacing_em,
        0.4
    );
    assert_eq!(
        resolve(json!({ "parentheticalIndentPct": 41 })).parenthetical_indent_pct,
        40.0
    );
    assert_eq!(resolve(json!({ "cueIndentPct": 61 })).cue_indent_pct, 60.0);
}

#[test]
fn unknown_keys_are_ignored() {
    assert_eq!(resolve(json!({ "nonsense": true })), DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn a_partial_that_is_not_an_object_is_treated_as_empty() {
    assert_eq!(resolve(json!([1, 2])), DEFAULT_FORMAT_OPTIONS);
    assert_eq!(resolve(json!("x")), DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn justify_text_defaults_to_false_and_accepts_a_boolean() {
    assert!(!resolve(json!({})).justify_text);
    assert!(resolve(json!({ "justifyText": true })).justify_text);
    assert!(!resolve(json!({ "justifyText": "nope" })).justify_text);
}

#[test]
fn print_split_minimums_defaults_to_true_and_accepts_a_boolean() {
    assert!(resolve(json!({})).print_split_minimums);
    assert!(!resolve(json!({ "printSplitMinimums": false })).print_split_minimums);
    // A falsy invalid value stays true under the type-guarded fallback.
    assert!(resolve(json!({ "printSplitMinimums": 0 })).print_split_minimums);
}

#[test]
fn preserve_font_shifts_defaults_to_true_and_accepts_a_boolean() {
    assert!(resolve(json!({})).preserve_font_shifts);
    assert!(!resolve(json!({ "preserveFontShifts": false })).preserve_font_shifts);
    assert!(resolve(json!({ "preserveFontShifts": 0 })).preserve_font_shifts);
}

#[test]
fn include_images_defaults_to_true_and_accepts_a_boolean() {
    assert!(resolve(json!({})).include_images);
    assert!(!resolve(json!({ "includeImages": false })).include_images);
    assert!(resolve(json!({ "includeImages": 0 })).include_images);
}

#[test]
fn promote_cover_image_defaults_to_true_and_accepts_a_boolean() {
    assert!(resolve(json!({})).promote_cover_image);
    assert!(!resolve(json!({ "promoteCoverImage": false })).promote_cover_image);
    assert!(resolve(json!({ "promoteCoverImage": 0 })).promote_cover_image);
}

#[test]
fn merges_over_a_supplied_base_not_just_the_defaults() {
    let base = FormatOptions {
        keep_speeches_whole: true,
        justify_text: true,
        ..DEFAULT_FORMAT_OPTIONS
    };
    let merged = resolve_over(json!({ "dialogueSideMarginPct": 9 }), &base);
    assert_eq!(merged.dialogue_side_margin_pct, 9.0);
    // fields absent from the partial come from the BASE
    assert!(merged.keep_speeches_whole);
    assert!(merged.justify_text);
}

#[test]
fn an_out_of_range_value_clamps_against_the_base_rather_than_being_taken_raw() {
    let base = FormatOptions {
        dialogue_side_margin_pct: 12.0,
        ..DEFAULT_FORMAT_OPTIONS
    };
    assert_eq!(
        resolve_over(json!({ "dialogueSideMarginPct": 999 }), &base).dialogue_side_margin_pct,
        30.0
    );
    assert_eq!(
        resolve_over(json!({ "dialogueSideMarginPct": "nope" }), &base).dialogue_side_margin_pct,
        12.0
    );
}

#[test]
fn an_explicit_side_by_side_overrides_a_sequential_base() {
    let base = FormatOptions {
        dual_dialogue: DualDialogue::Sequential,
        ..DEFAULT_FORMAT_OPTIONS
    };
    assert_eq!(
        resolve_over(json!({ "dualDialogue": "sideBySide" }), &base).dual_dialogue,
        DualDialogue::SideBySide
    );
}

#[test]
fn an_explicit_sequential_still_overrides_a_side_by_side_base() {
    let base = FormatOptions {
        dual_dialogue: DualDialogue::SideBySide,
        ..DEFAULT_FORMAT_OPTIONS
    };
    assert_eq!(
        resolve_over(json!({ "dualDialogue": "sequential" }), &base).dual_dialogue,
        DualDialogue::Sequential
    );
}

#[test]
fn an_invalid_dual_dialogue_value_falls_back_to_the_base() {
    let base = FormatOptions {
        dual_dialogue: DualDialogue::Sequential,
        ..DEFAULT_FORMAT_OPTIONS
    };
    assert_eq!(
        resolve_over(json!({ "dualDialogue": "nonsense" }), &base).dual_dialogue,
        DualDialogue::Sequential
    );
}

#[test]
fn the_string_enums_take_each_valid_value_and_fall_back_on_anything_else() {
    assert_eq!(
        resolve(json!({ "fontFamily": "serif" })).font_family,
        FontFamily::Serif
    );
    assert_eq!(
        resolve(json!({ "fontFamily": "sans" })).font_family,
        FontFamily::Sans
    );
    assert_eq!(
        resolve(json!({ "fontFamily": "Serif" })).font_family,
        FontFamily::Courier
    );
    assert_eq!(
        resolve(json!({ "contdMode": "strip" })).contd_mode,
        ContdMode::Strip
    );
    assert_eq!(
        resolve(json!({ "contdMode": "keep" })).contd_mode,
        ContdMode::Keep
    );
    assert_eq!(
        resolve(json!({ "contdMode": 1 })).contd_mode,
        ContdMode::Auto
    );
    assert_eq!(
        resolve(json!({ "cueAlignment": "indented" })).cue_alignment,
        CueAlignment::Indented
    );
    assert_eq!(
        resolve(json!({ "cueAlignment": "left" })).cue_alignment,
        CueAlignment::Centered
    );
}

#[test]
fn keep_speeches_whole_defaults_off_and_resolves_from_partials() {
    assert!(!resolve(json!({})).keep_speeches_whole);
    assert!(resolve(json!({ "keepSpeechesWhole": true })).keep_speeches_whole);
    assert!(!resolve(json!({ "keepSpeechesWhole": "yes" })).keep_speeches_whole);
}
