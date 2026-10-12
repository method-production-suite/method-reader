//! Port of tests/presets.test.ts.

use sp_model::{
    DEFAULT_FORMAT_OPTIONS, DevicePresetId, DualDialogue, FormatOptions, device_preset,
    matching_preset,
};

#[test]
fn the_kindle_eink_preset_is_exactly_the_defaults() {
    assert_eq!(
        device_preset(DevicePresetId::KindleEink).settings,
        DEFAULT_FORMAT_OPTIONS
    );
}

// "a preset does not alias the module-level defaults" has no Rust form:
// `device_preset` returns a copy, so there is no shared object to alias.

#[test]
fn the_phone_preset_widens_the_column_and_drops_side_by_side_dual_dialogue() {
    assert_eq!(
        device_preset(DevicePresetId::Phone).settings,
        FormatOptions {
            dialogue_side_margin_pct: 10.0,
            dual_dialogue: DualDialogue::Sequential,
            ..DEFAULT_FORMAT_OPTIONS
        }
    );
}

#[test]
fn matching_preset_names_the_preset_whose_settings_match_exactly() {
    assert_eq!(
        matching_preset(&DEFAULT_FORMAT_OPTIONS),
        Some(DevicePresetId::KindleEink)
    );
    assert_eq!(
        matching_preset(&device_preset(DevicePresetId::Phone).settings),
        Some(DevicePresetId::Phone)
    );
}

#[test]
fn matching_preset_returns_none_once_a_knob_is_tuned_away_from_every_preset() {
    let tuned = FormatOptions {
        cue_indent_pct: 41.0,
        ..DEFAULT_FORMAT_OPTIONS
    };
    assert_eq!(matching_preset(&tuned), None);
}

#[test]
fn every_preset_has_a_display_name() {
    for id in DevicePresetId::ALL {
        assert!(!device_preset(id).display_name.is_empty());
    }
}

#[test]
fn preset_ids_spell_as_the_typescript_keys() {
    assert_eq!(
        serde_json::to_string(&DevicePresetId::KindleEink).unwrap(),
        "\"kindleEink\""
    );
    assert_eq!(
        serde_json::to_string(&DevicePresetId::Phone).unwrap(),
        "\"phone\""
    );
}
