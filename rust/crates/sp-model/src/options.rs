//! Port of `src/options.ts`: the single knob surface shared by the CLI
//! (`--options file.json`) and the window's Settings page. The registry with
//! the rationale for each knob is docs/formatting-options-log.md.
//!
//! The defaults are pinned to the repo's root `format-defaults.json` by
//! `tests/format_defaults.rs`, as `tests/options.test.ts` pins the
//! TypeScript's: change one and the other together.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Body typeface (registry #6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontFamily {
    Courier,
    Serif,
    Sans,
}

/// (CONT'D) handling (registry #8a): `auto` strips stale ones and re-adds
/// per the standard rule, `strip` removes all, `keep` leaves them as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContdMode {
    Auto,
    Strip,
    Keep,
}

/// Cue and parenthetical alignment (registry #2b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CueAlignment {
    Centered,
    Indented,
}

/// Simultaneous speech (registry #10a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DualDialogue {
    SideBySide,
    Sequential,
}

/// `FormatOptions`. Field order is the TypeScript's, which is also the order
/// `format-defaults.json` and the CLI's JSON write them in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOptions {
    /// Start each scene on a new page (registry #1).
    pub scene_page_breaks: bool,
    /// Dialogue column side margins, % of screen (registry #2).
    #[serde(with = "crate::js::number")]
    pub dialogue_side_margin_pct: f64,
    /// Cue indent, % of the dialogue column (registry #2).
    #[serde(with = "crate::js::number")]
    pub cue_indent_pct: f64,
    /// Parenthetical indent, % of the dialogue column (registry #2).
    #[serde(with = "crate::js::number")]
    pub parenthetical_indent_pct: f64,
    /// Blank space between elements, em (registry #3).
    #[serde(with = "crate::js::number")]
    pub element_spacing_em: f64,
    /// Heading holds to what follows via `break-after: avoid` (registry #5a).
    pub keep_scene_heading_with_scene: bool,
    /// Whole speeches ride one page (registry #8c).
    pub keep_speeches_whole: bool,
    /// Body typeface (registry #6).
    pub font_family: FontFamily,
    /// Merge (MORE)/(CONT'D) page-break splits into one speech (registry #8).
    pub rejoin_split_dialogue: bool,
    /// (CONT'D) handling (registry #8a).
    pub contd_mode: ContdMode,
    /// Cue/parenthetical alignment (registry #2b).
    pub cue_alignment: CueAlignment,
    /// Generated title page (registry #11).
    pub include_title_page: bool,
    /// Shooting-script scene numbers in headings (registry #13).
    pub show_scene_numbers: bool,
    /// Original PDF page-number markers at page boundaries (registry #13a).
    pub show_page_markers: bool,
    /// Side-by-side table or sequential speeches (registry #10a).
    pub dual_dialogue: DualDialogue,
    /// Justify body text rather than ragged-right (registry #6b).
    pub justify_text: bool,
    /// Widows/orphans 2 rather than 1 (registry #17).
    pub print_split_minimums: bool,
    /// Render block-level font shifts the PDF carried (registry #18).
    pub preserve_font_shifts: bool,
    /// Render the pictures the PDF carried (registry #19).
    pub include_images: bool,
    /// Make a designed title card the book's cover (registry #19b).
    pub promote_cover_image: bool,
}

/// `DEFAULT_FORMAT_OPTIONS`.
pub const DEFAULT_FORMAT_OPTIONS: FormatOptions = FormatOptions {
    scene_page_breaks: false,
    dialogue_side_margin_pct: 20.0,
    cue_indent_pct: 33.0,
    parenthetical_indent_pct: 17.0,
    element_spacing_em: 1.0,
    keep_scene_heading_with_scene: true,
    keep_speeches_whole: false,
    font_family: FontFamily::Courier,
    rejoin_split_dialogue: true,
    contd_mode: ContdMode::Auto,
    cue_alignment: CueAlignment::Centered,
    include_title_page: true,
    show_scene_numbers: false,
    show_page_markers: true,
    dual_dialogue: DualDialogue::SideBySide,
    justify_text: false,
    print_split_minimums: true,
    preserve_font_shifts: true,
    include_images: true,
    promote_cover_image: true,
};

impl Default for FormatOptions {
    fn default() -> Self {
        DEFAULT_FORMAT_OPTIONS
    }
}

/// `Math.min(hi, Math.max(lo, v))`.
fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    hi.min(lo.max(v))
}

/// `resolveFormatOptions(partial, base)`: merge a partial (as parsed from
/// `--options` JSON) over `base`, clamping numeric knobs and ignoring
/// unknown keys and values of the wrong type. A `partial` that is absent or
/// not a JSON object is treated as `{}`.
///
/// `base` is the shipped defaults for a plain conversion; the per-script
/// sidecar passes the user's global settings instead, so a sidecar carrying
/// one key overrides one knob and leaves the rest of their tuning alone.
/// Pass [`DEFAULT_FORMAT_OPTIONS`] for the TypeScript's default argument.
pub fn resolve_format_options(partial: Option<&Value>, base: &FormatOptions) -> FormatOptions {
    let empty = serde_json::Map::new();
    let p = match partial {
        Some(Value::Object(map)) => map,
        _ => &empty,
    };
    let d = base;
    // `typeof p[key] === 'boolean' ? p[key] : d[key]`
    let bool_or =
        |key: &str, fallback: bool| p.get(key).and_then(Value::as_bool).unwrap_or(fallback);
    // `typeof p[key] === 'number' && Number.isFinite(p[key]) ? clamp(...) : d[key]`
    let num_or = |key: &str, lo: f64, hi: f64, fallback: f64| match p.get(key) {
        Some(Value::Number(n)) => match n.as_f64() {
            Some(v) if v.is_finite() => clamp(v, lo, hi),
            _ => fallback,
        },
        _ => fallback,
    };
    let str_of = |key: &str| p.get(key).and_then(Value::as_str);

    FormatOptions {
        scene_page_breaks: bool_or("scenePageBreaks", d.scene_page_breaks),
        dialogue_side_margin_pct: num_or(
            "dialogueSideMarginPct",
            0.0,
            30.0,
            d.dialogue_side_margin_pct,
        ),
        cue_indent_pct: num_or("cueIndentPct", 0.0, 60.0, d.cue_indent_pct),
        parenthetical_indent_pct: num_or(
            "parentheticalIndentPct",
            0.0,
            40.0,
            d.parenthetical_indent_pct,
        ),
        element_spacing_em: num_or("elementSpacingEm", 0.4, 2.0, d.element_spacing_em),
        keep_scene_heading_with_scene: bool_or(
            "keepSceneHeadingWithScene",
            d.keep_scene_heading_with_scene,
        ),
        keep_speeches_whole: bool_or("keepSpeechesWhole", d.keep_speeches_whole),
        font_family: match str_of("fontFamily") {
            Some("serif") => FontFamily::Serif,
            Some("sans") => FontFamily::Sans,
            Some("courier") => FontFamily::Courier,
            _ => d.font_family,
        },
        rejoin_split_dialogue: bool_or("rejoinSplitDialogue", d.rejoin_split_dialogue),
        contd_mode: match str_of("contdMode") {
            Some("strip") => ContdMode::Strip,
            Some("keep") => ContdMode::Keep,
            Some("auto") => ContdMode::Auto,
            _ => d.contd_mode,
        },
        cue_alignment: match str_of("cueAlignment") {
            Some("indented") => CueAlignment::Indented,
            Some("centered") => CueAlignment::Centered,
            _ => d.cue_alignment,
        },
        include_title_page: bool_or("includeTitlePage", d.include_title_page),
        show_scene_numbers: bool_or("showSceneNumbers", d.show_scene_numbers),
        show_page_markers: bool_or("showPageMarkers", d.show_page_markers),
        dual_dialogue: match str_of("dualDialogue") {
            Some("sequential") => DualDialogue::Sequential,
            Some("sideBySide") => DualDialogue::SideBySide,
            _ => d.dual_dialogue,
        },
        justify_text: bool_or("justifyText", d.justify_text),
        print_split_minimums: bool_or("printSplitMinimums", d.print_split_minimums),
        preserve_font_shifts: bool_or("preserveFontShifts", d.preserve_font_shifts),
        include_images: bool_or("includeImages", d.include_images),
        promote_cover_image: bool_or("promoteCoverImage", d.promote_cover_image),
    }
}
