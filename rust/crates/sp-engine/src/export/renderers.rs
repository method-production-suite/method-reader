//! What the export path needs from the crates that make books, as one
//! seam: the AZW3 (from the EPUB's bytes, sp-azw3, with its writer
//! revision for the staleness rule), the MOBI (from the `.fountain`,
//! through [`crate::convert::convert_fountain`]), the FDX (sp-fdx), the OSF
//! (sp-osf) and the note stripping (sp-fountain).
//!
//! [`Books`] is the real thing, and what every caller passes. The trait
//! stays so the ladder's own tests can hand it stand-in book makers and pin
//! the ladder's decisions apart from the books'.

use std::collections::BTreeMap;

use sp_model::FormatOptions;

use crate::convert::{ConvertOptions, convert_fountain};

/// The book makers the export path calls.
pub trait Renderers {
    /// `AZW3_WRITER_REVISION`.
    fn azw3_writer_revision(&self) -> u32;
    /// `buildAzw3(epubBytes)`.
    fn build_azw3(&self, epub: &[u8]) -> Result<Vec<u8>, String>;
    /// `convertFountain(text, { format, mobi: true, images }).mobi`:
    /// `Ok(None)` when the engine reported success and made no .mobi.
    fn render_mobi(
        &self,
        fountain: &str,
        format: &FormatOptions,
        images: Option<&BTreeMap<String, Vec<u8>>>,
    ) -> Result<Option<Vec<u8>>, String>;

    /// `fountainToFdx(text)`: fountain-js's tokens, then sp-fdx.
    fn fountain_to_fdx(&self, fountain: &str) -> Result<String, String> {
        let tokens = sp_fountain::tokenize(fountain).map_err(|e| e.to_string())?;
        Ok(sp_fdx::tokens_to_fdx(&tokens))
    }

    /// The Open Screenplay Format document: fountain-js's tokens, then
    /// sp-osf. The same tokens the FDX is written from.
    fn fountain_to_osf(&self, fountain: &str) -> Result<String, String> {
        let tokens = sp_fountain::tokenize(fountain).map_err(|e| e.to_string())?;
        Ok(sp_osf::tokens_to_osf(&tokens))
    }

    /// `stripScreepubNotes(text)`.
    fn strip_screepub_notes(&self, fountain: &str) -> Result<String, String> {
        Ok(sp_fountain::notes::strip_screepub_notes(fountain))
    }
}

/// The real book makers.
#[derive(Debug, Clone, Copy, Default)]
pub struct Books;

impl Renderers for Books {
    fn azw3_writer_revision(&self) -> u32 {
        sp_azw3::AZW3_WRITER_REVISION
    }

    fn build_azw3(&self, epub: &[u8]) -> Result<Vec<u8>, String> {
        sp_azw3::build_azw3(epub).map_err(|e| e.to_string())
    }

    fn render_mobi(
        &self,
        fountain: &str,
        format: &FormatOptions,
        images: Option<&BTreeMap<String, Vec<u8>>>,
    ) -> Result<Option<Vec<u8>>, String> {
        let opts = ConvertOptions {
            format: *format,
            mobi: true,
            ..ConvertOptions::default()
        };
        convert_fountain(fountain, opts, images)
            .map(|r| r.mobi)
            .map_err(|e| e.to_string())
    }
}
