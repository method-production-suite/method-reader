//! The `= pg N` page marker, read back: the label of the PDF page a stretch
//! of the .fountain sat on (registry #13a). serialize.rs writes the marker,
//! a synopsis fountain-js tokenizes like any other; this is the one reader
//! sp-fdx asks for each scene's starting page. The label is the page number
//! as PRINTED when the PDF numbers its pages (serialize's
//! `printed_page_offset`), else the sheet number.
//!
//! The EPUB and MOBI renderers predate this module and carry the same
//! pattern themselves (`/^pg\s+(\S+)$/` on the trimmed text); they are
//! left alone so their frozen books cannot move.

use sp_model::{Token, TokenType};

use crate::js;

/// The label a page-marker synopsis names, or `None` for any other
/// synopsis: `/^pg\s+(\S+)$/` on the synopsis's trimmed text.
pub fn page_marker_label(synopsis: &str) -> Option<&str> {
    let rest = js::trim(synopsis).strip_prefix("pg")?;
    if !rest.starts_with(js::is_space) {
        return None;
    }
    let label = js::trim_start(rest);
    (!label.is_empty() && !label.contains(js::is_space)).then_some(label)
}

/// The label of the page before the first `= pg N` marker: the page before
/// the one that marker names, when it names a number above 1 (the
/// serializer writes no marker for the page a script opens on). `None`
/// without markers, or when the first one names a page that is not a
/// number.
pub fn opening_page(tokens: &[Token]) -> Option<String> {
    let first = tokens.iter().find_map(|t| {
        (t.kind == TokenType::Synopsis)
            .then(|| page_marker_label(t.text_or_empty()))
            .flatten()
    })?;
    let n: i64 = first.parse().ok()?;
    (n > 1).then(|| (n - 1).to_string())
}
