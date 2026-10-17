//! Method Reader's shared data model, ported from the TypeScript engine.
//!
//! | Module | Ports |
//! |---|---|
//! | [`types`] | `src/parser/types.ts`: every element kind, line, block and image shape |
//! | [`options`] | `src/options.ts`: [`FormatOptions`], its defaults and `resolveFormatOptions` |
//! | [`presets`] | `src/settings/presets.ts`: the named device bundles of `FormatOptions` |
//! | [`token`] | the token shape `fountain-js` 1.2.4 hands every renderer |
//! | [`js`] | serde helpers that write numbers the way `JSON.stringify` does |
//!
//! Every type derives serde with the field names the TypeScript writes, so
//! an oracle dump (`rust/oracle/<fixture>/*.json`) deserializes directly.
//! Comparisons against the oracle are SEMANTIC (parsed JSON), never on key
//! order: JavaScript orders an object's keys by when they were assigned,
//! which no struct can promise to mirror for optional fields.

pub mod js;
pub mod options;
pub mod presets;
pub mod token;
pub mod types;

pub use options::{
    ContdMode, CueAlignment, DEFAULT_FORMAT_OPTIONS, DualDialogue, FontFamily, FormatOptions,
    resolve_format_options,
};
pub use presets::{DevicePreset, DevicePresetId, device_preset, matching_preset};
pub use token::{Dual, Token, TokenType};
pub use types::*;
