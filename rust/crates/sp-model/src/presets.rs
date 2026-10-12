//! Port of `src/settings/presets.ts`: named bundles of [`FormatOptions`]
//! tuned for a class of reading device. Responsive reflow is impossible in a
//! fixed e-book (no JS, media queries stripped), so a preset chosen at
//! conversion time is the mechanism for device-appropriate geometry.

use serde::{Deserialize, Serialize};

use crate::options::{DEFAULT_FORMAT_OPTIONS, DualDialogue, FormatOptions};

/// `DevicePresetId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DevicePresetId {
    KindleEink,
    Phone,
}

impl DevicePresetId {
    /// Every preset, in the TypeScript's key order (which `matchingPreset`
    /// searches in).
    pub const ALL: [DevicePresetId; 2] = [DevicePresetId::KindleEink, DevicePresetId::Phone];
}

/// One entry of `DEVICE_PRESETS`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePreset {
    pub display_name: &'static str,
    pub settings: FormatOptions,
}

/// `DEVICE_PRESETS[id]`. Returned by value, so no caller can corrupt the
/// defaults through a preset (the hazard the TypeScript copies to avoid).
pub fn device_preset(id: DevicePresetId) -> DevicePreset {
    match id {
        // The recommended baseline: 6" e-ink Kindle. Identical to defaults.
        DevicePresetId::KindleEink => DevicePreset {
            display_name: "Kindle e-ink (6\")",
            settings: DEFAULT_FORMAT_OPTIONS,
        },
        // Narrow phone/tablet reading app: side-by-side dual dialogue is an
        // unreadable sliver, so speeches go sequential, and the dialogue
        // column widens to use the small screen.
        DevicePresetId::Phone => DevicePreset {
            display_name: "Phone / narrow screen",
            settings: FormatOptions {
                dialogue_side_margin_pct: 10.0,
                dual_dialogue: DualDialogue::Sequential,
                ..DEFAULT_FORMAT_OPTIONS
            },
        },
    }
}

/// `matchingPreset`: the preset whose settings exactly match, or `None` when
/// they have been tuned away from every preset.
pub fn matching_preset(settings: &FormatOptions) -> Option<DevicePresetId> {
    DevicePresetId::ALL
        .into_iter()
        .find(|&id| device_preset(id).settings == *settings)
}
