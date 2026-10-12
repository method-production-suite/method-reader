//! Port of `src/parser/types.ts`: what the parser reads and what it makes.
//!
//! Field names are the TypeScript's (camelCase on the wire). Optional
//! TypeScript fields are `Option`s that are left out when `None`, exactly as
//! `JSON.stringify` leaves out an `undefined` property. Counts and page
//! numbers are integers here; every coordinate and percentage is an `f64`
//! written the JavaScript way (see [`crate::js`]).

use serde::{Deserialize, Serialize};

/// `ElementType`: what a classified block of the script is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElementType {
    Scene,
    Character,
    Dialogue,
    Action,
    Parenthetical,
    Transition,
    PageNumber,
    MiniSlug,
    Image,
}

/// `FamilyBucket`: coarse font-family buckets. Deliberately four: a
/// screenplay's own face, and the three kinds of thing a chyron, insert or
/// letter is ever set in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FamilyBucket {
    Mono,
    Serif,
    Sans,
    Cursive,
}

/// `SizeStep`: coarse size steps relative to the document's dominant size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SizeStep {
    #[serde(rename = "-1")]
    Smaller,
    #[serde(rename = "+1")]
    Larger,
    #[serde(rename = "+2")]
    MuchLarger,
}

/// `Fmt`: a block-level font shift. The parser always sets at least one
/// field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Fmt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<FamilyBucket>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<SizeStep>,
}

/// `FontRun`: characters of one line set in one (bucket, size) pair. Runs
/// whose font never resolved are omitted, so shares are over resolved
/// characters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FontRun {
    pub bucket: FamilyBucket,
    #[serde(with = "crate::js::number")]
    pub size: f64,
    pub chars: u32,
}

/// `ScreenplayElement.ageValue`, which the TypeScript types `string | number`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgeValue {
    Number(#[serde(with = "crate::js::number")] f64),
    Text(String),
}

/// `ScreenplayElement`: one classified element of the script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenplayElement {
    #[serde(rename = "type")]
    pub kind: ElementType,
    pub text: String,
    pub id: String,
    pub page_num: u32,
    pub is_title_page: bool,
    pub is_readable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_character: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_modifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_value: Option<AgeValue>,
    /// Scene number from shooting scripts (e.g. "2.2.", "1A."), on scene
    /// elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_number: Option<String>,
    /// Text with fountain emphasis markers from PDF font styles (`*i*`,
    /// `**b**`). Only dialogue and action carry one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styled_text: Option<String>,
    /// Cue of the RIGHT column of a simultaneous (dual) exchange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_right: Option<bool>,
    /// Block-level font shift, serialized as a `[[fmt: ...]]` note (#18).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fmt: Option<Fmt>,
    /// Image carried out of the PDF, serialized as an `[[img: ...]]` note (#19).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub img: Option<ImageRef>,
    /// Where this element sat on its page, in PDF user space (up is the high
    /// y). A text element carries the BASELINE of its block's topmost line,
    /// an image element its own top edge. Optional only because hand-built
    /// test elements omit it; the parser always sets it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::js::opt_number"
    )]
    pub y: Option<f64>,
}

/// `ImageRef`: an image as the .fountain and the renderers see it (#19).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRef {
    /// Filename inside the picture folder (`<stem>.images/`).
    pub file: String,
    /// Rendered width as a percentage of the script's TEXT COLUMN (not of the
    /// page, which is `RawImage::page_width_pct`), clamped to 5..100.
    #[serde(with = "crate::js::number")]
    pub width_pct: f64,
    /// Covered at least `FULL_PAGE_AREA_FRAC` of its page in the source PDF.
    pub full_page: bool,
}

/// `CharacterInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterInfo {
    pub name: String,
    pub base_name: String,
    pub dialogue_count: u32,
    pub first_appearance: u32,
}

/// `SceneInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneInfo {
    pub heading: String,
    pub scene_number: u32,
    pub element_index: u32,
    pub page_num: u32,
}

/// `ParsedScreenplay`: the parser's whole answer, and the shape of the
/// oracle's `elements.json` (the CLI's `--debug` dump).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedScreenplay {
    pub elements: Vec<ScreenplayElement>,
    pub characters: Vec<CharacterInfo>,
    pub scenes: Vec<SceneInfo>,
    pub page_count: u32,
}

/// `RawLine`: one line extracted from a PDF page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawLine {
    pub text: String,
    /// 0-100, a percentage of page width.
    #[serde(with = "crate::js::number")]
    pub indent: f64,
    #[serde(with = "crate::js::number")]
    pub y: f64,
    pub page_num: u32,
    /// Text with fountain emphasis markers, present only when styles differ.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styled: Option<String>,
    /// Cue line opening the right column of a dual-dialogue region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_right: Option<bool>,
    /// Per-run font tallies, consumed by the document-dominant pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonts: Option<Vec<FontRun>>,
    /// Block-level font shift relative to the document's dominant font.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fmt: Option<Fmt>,
}

/// `RawImage`: an image carried out of the PDF, positioned alongside
/// [`RawLine`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawImage {
    /// Filename in the picture folder, assigned once where the image is
    /// numbered (`p001-01.png`) and never derived again.
    pub file: String,
    pub page_num: u32,
    /// TOP edge in PDF user space, same axis and direction as `RawLine.y`.
    #[serde(with = "crate::js::number")]
    pub y: f64,
    /// Rendered size as a percentage of PAGE width and height.
    #[serde(with = "crate::js::number")]
    pub page_width_pct: f64,
    #[serde(with = "crate::js::number")]
    pub page_height_pct: f64,
    /// The encoded PNG. Not in JSON: `JSON.stringify` of a `Uint8Array` is
    /// an index-keyed object nobody reads, so the oracle writes each PNG as
    /// its own file (`images/<file>`) and its JSON carries `sha256` instead.
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

/// `TextBlock`: lines grouped into one block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBlock {
    pub lines: Vec<RawLine>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styled_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fmt: Option<Fmt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_right: Option<bool>,
    /// First line's indent.
    #[serde(with = "crate::js::number")]
    pub indent: f64,
    #[serde(with = "crate::js::number")]
    pub min_indent: f64,
    #[serde(with = "crate::js::number")]
    pub max_indent: f64,
    pub page_num: u32,
    #[serde(with = "crate::js::number")]
    pub y_position: f64,
}

/// `INDENT_RANGES`: indent bands (percent of page width) for classification.
pub mod indent_ranges {
    pub const ACTION_MAX: f64 = 20.0;
    pub const DIALOGUE_MIN: f64 = 25.0;
    pub const DIALOGUE_MAX: f64 = 35.0;
    pub const CHARACTER_MIN: f64 = 35.0;
    pub const CHARACTER_MAX: f64 = 50.0;
    pub const TRANSITION_MIN: f64 = 55.0;
}
