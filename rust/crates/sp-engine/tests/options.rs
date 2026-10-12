//! The convertFountain block of tests/options.test.ts: format options carried
//! end to end into the EPUB. The rest of that file is sp-model's, sp-fountain's
//! and sp-epub's (rust/PARITY.md).

mod common;

use sp_engine::convert::{ConvertOptions, convert_fountain};
use sp_model::DEFAULT_FORMAT_OPTIONS;

#[test]
fn custom_geometry_reaches_style_css() {
    let mut format = DEFAULT_FORMAT_OPTIONS;
    format.dialogue_side_margin_pct = 28.0;
    format.scene_page_breaks = true;
    let result = convert_fountain(
        "INT. A - DAY\n\nText.\n",
        ConvertOptions {
            format,
            ..ConvertOptions::default()
        },
        None,
    )
    .unwrap();
    let css = common::epub::text(&result.epub, "OEBPS/style.css");
    assert!(css.contains("margin-left: 28%"));
    assert!(css.contains("section.scene { page-break-before: always; }"));
}
