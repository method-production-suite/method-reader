//! Method Reader's own AZW3 (KF8) writer: the EPUB's own bytes in, a Kindle
//! book out, with no Calibre. A port of Calibre 9.11.0's KF8 writer
//! (src/calibre/ebooks/mobi/writer8/, GPL-3, Copyright Kovid Goyal),
//! credited in each module and in THIRD-PARTY-NOTICES.md; this port is licensed
//! GPL-3.0-only, the one crate not MIT OR Apache-2.0 (LICENSE-GPL-3.0). BETA until a Kindle has opened one
//! (registry #20).
//!
//! | Module | Ports |
//! |---|---|
//! | [`writer`] | `src/azw3/writer.ts`: the file, record by record |
//! | [`chunk`] | `src/azw3/chunk.ts`: KF8 markup, skeletons and fragments |
//! | [`index`] | `src/azw3/index.ts`: INDX, CNCX and the number encodings |
//! | [`tbs`] | `src/azw3/tbs.ts`: trailing byte sequences |
//! | [`palmdoc`] | `src/azw3/palmdoc.ts`: PalmDOC compression |
//! | [`epub`] | `src/azw3/epub.ts`: the EPUB read back out of its zip |
//! | [`xml`] | `src/azw3/xml.ts`: the small XML reader and writer |
//! | [`revision`] | `src/azw3/revision.ts`: the writer revision, EXTH 0x5350 |
//!
//! The PalmDB container is sp-mobi's, shared with the MOBI 6 writer.

pub mod chunk;
pub mod epub;
pub mod index;
pub mod palmdoc;
pub mod revision;
pub mod tbs;
pub mod writer;
pub mod xml;

use std::fmt;

pub use revision::{
    AZW3_REVISION_EXTH, AZW3_WRITER_REVISION, azw3_revision, azw3_revision_of_file,
};
pub use writer::{build_azw3, build_azw3_at, build_azw3_from_book};

/// Why an AZW3 could not be built: the EPUB is not one the writer reads
/// (the TypeScript threw with the same words).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Azw3Error(pub String);

impl Azw3Error {
    pub fn new(message: impl Into<String>) -> Azw3Error {
        Azw3Error(message.into())
    }
}

impl fmt::Display for Azw3Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Azw3Error {}
