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
