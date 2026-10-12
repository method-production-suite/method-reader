//! What the export path needs from the crates that make books, as one
//! seam.
//!
//! The export verb builds three files from text the engine already has:
//! the AZW3 (from the EPUB's bytes, sp-azw3), the MOBI (from the
//! `.fountain`, through the conversion: sp-mobi and convert.ts's
//! `convertFountain`, which is wave 3's), and the FDX (from the `.fountain`,
//! sp-fdx). It also strips Screepub's own notes from a `.fountain` it hands
//! over (sp-fountain's `strip_screepub_notes`).
//!
//! sp-fountain and sp-fdx have landed, so the FDX and the note stripping
//! are the real crates', as provided methods every implementation shares.
//! The AZW3 writer and the MOBI renderer are still being ported, so the
//! export takes them through this trait and the CLI (wave 3) passes the
//! real ones; [`Unported`] stands in until then, refusing each build with a
//! sentence that says so.
//!
//! The AZW3 writer's REVISION is part of this seam too: the staleness rule
//! counts an `.azw3` current only when it carries this writer's revision or
//! a later one (azw3/revision.ts), and that number is sp-azw3's to bump.
//! Reading a revision out of a file is the small reader in
//! [`super::azw3_revision`].

use std::collections::BTreeMap;

use sp_model::FormatOptions;

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

    /// `stripScreepubNotes(text)`.
    fn strip_screepub_notes(&self, fountain: &str) -> Result<String, String> {
        Ok(sp_fountain::notes::strip_screepub_notes(fountain))
    }
}

/// The stand-in until sp-azw3, sp-mobi and the conversion land. Its
/// revision is the TypeScript's `AZW3_WRITER_REVISION` today, 1.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unported;

fn not_yet(what: &str, krate: &str) -> String {
    format!("the {what} is not ported yet ({krate})")
}

impl Renderers for Unported {
    fn azw3_writer_revision(&self) -> u32 {
        1
    }

    fn build_azw3(&self, _epub: &[u8]) -> Result<Vec<u8>, String> {
        Err(not_yet("AZW3 writer", "sp-azw3"))
    }

    fn render_mobi(
        &self,
        _fountain: &str,
        _format: &FormatOptions,
        _images: Option<&BTreeMap<String, Vec<u8>>>,
    ) -> Result<Option<Vec<u8>>, String> {
        Err(not_yet("MOBI renderer", "sp-mobi and convertFountain"))
    }
}
