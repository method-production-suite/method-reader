//! Port of `src/export/freshness.ts`: whether a file derived from the EPUB
//! is current with it.

use crate::fsx;

/// `modifiedDate(path)`: when `path` was last written, in ms since the
/// epoch, or `None` when it cannot be read.
pub fn modified_date(path: &str) -> Option<f64> {
    fsx::mtime_ms(path)
}

/// `needsRegeneration(artifact, freshRelativeTo)`: true when `artifact` is
/// missing, or no newer than the EPUB it derives from. Ties count as
/// stale. Both fallbacks fail toward "regenerate": a missing artifact
/// directly, and an unreadable EPUB by reading as +Infinity, so a stale
/// artifact is never reported fresh because its EPUB could not be read.
pub fn needs_regeneration(artifact: &str, fresh_relative_to: &str) -> bool {
    let Some(artifact_date) = fsx::mtime_ms(artifact) else {
        return true;
    };
    let epub_date = fsx::mtime_ms(fresh_relative_to).unwrap_or(f64::INFINITY);
    artifact_date <= epub_date
}
