//! Stand-ins for the book makers the export reaches through
//! `Renderers` (sp-azw3 and the MOBI conversion are being ported in
//! parallel), and tests/helpers/stamped-azw3.ts.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use sp_engine::export::renderers::Renderers;
use sp_model::FormatOptions;

/// The smallest file the AZW3 staleness rule reads as one of ours: a
/// PalmDB whose record 0 is a MOBI header with an EXTH block carrying the
/// writer revision. `None`: the same file with no revision record, as
/// Calibre's AZW3 looks.
pub fn stamped_azw3(revision: Option<u32>) -> Vec<u8> {
    let mut records: Vec<Vec<u8>> = Vec::new();
    // EXTH 503 (the title), then the revision.
    let mut title = Vec::new();
    title.extend(503u32.to_be_bytes());
    title.extend(9u32.to_be_bytes());
    title.push(b'T');
    records.push(title);
    if let Some(rev) = revision {
        let mut r = Vec::new();
        r.extend(0x5350u32.to_be_bytes());
        r.extend(12u32.to_be_bytes());
        r.extend(rev.to_be_bytes());
        records.push(r);
    }
    let size: usize = records.iter().map(Vec::len).sum();
    let mut exth = b"EXTH".to_vec();
    exth.extend(((size + 12) as u32).to_be_bytes());
    exth.extend((records.len() as u32).to_be_bytes());
    for r in &records {
        exth.extend(r);
    }
    exth.extend(vec![0u8; 4 - size % 4]);

    let mut r0 = vec![0u8; 16];
    r0[0..2].copy_from_slice(&2u16.to_be_bytes());
    r0.extend(b"MOBI");
    r0.extend(264u32.to_be_bytes());
    r0.extend(2u32.to_be_bytes());
    r0.extend(65001u32.to_be_bytes());
    r0.extend(1u32.to_be_bytes());
    r0.extend(8u32.to_be_bytes());
    r0.resize(128, 0);
    r0.extend(0x50u32.to_be_bytes());
    r0.resize(16 + 264, 0);
    r0.extend(exth);

    let mut file = vec![0u8; 78];
    file[..1].copy_from_slice(b"T");
    file[60..68].copy_from_slice(b"BOOKMOBI");
    file[76..78].copy_from_slice(&1u16.to_be_bytes());
    let r0_at: u32 = 78 + 8 + 2;
    file.extend(r0_at.to_be_bytes());
    file.extend([0, 0, 0, 0]);
    file.extend([0, 0]);
    file.extend(r0);
    file
}

/// Book makers that make stand-ins: an AZW3 is [`stamped_azw3`] of this
/// revision (refusing an EPUB that is not a zip, as the real writer does),
/// and a MOBI names the pictures it was handed, so a test can see them.
pub struct FakeRenderers {
    pub revision: u32,
    /// Every call to `render_mobi`: the pictures it was handed (`None`:
    /// the folder was not read).
    pub mobi_calls: RefCell<Vec<Option<Vec<String>>>>,
    pub azw3_calls: RefCell<usize>,
}

impl Default for FakeRenderers {
    fn default() -> Self {
        FakeRenderers {
            revision: 1,
            mobi_calls: RefCell::new(Vec::new()),
            azw3_calls: RefCell::new(0),
        }
    }
}

impl Renderers for FakeRenderers {
    fn azw3_writer_revision(&self) -> u32 {
        self.revision
    }

    fn build_azw3(&self, epub: &[u8]) -> Result<Vec<u8>, String> {
        *self.azw3_calls.borrow_mut() += 1;
        if !epub.starts_with(b"PK") {
            return Err("End of central directory record signature not found".into());
        }
        Ok(stamped_azw3(Some(self.revision)))
    }

    fn render_mobi(
        &self,
        _fountain: &str,
        _format: &FormatOptions,
        images: Option<&BTreeMap<String, Vec<u8>>>,
    ) -> Result<Option<Vec<u8>>, String> {
        let names = images.map(|m| m.keys().cloned().collect::<Vec<_>>());
        self.mobi_calls.borrow_mut().push(names.clone());
        Ok(Some(
            format!("MOBI {}", names.map(|n| n.join(",")).unwrap_or_default()).into_bytes(),
        ))
    }
}

/// A file that starts like a zip: what the fake AZW3 writer accepts.
pub const FAKE_EPUB: &[u8] = b"PK\x03\x04 a stand-in book";

/// `utimesSync(path, at, at)`, `age` seconds ago (negative: in the future).
pub fn age(path: &str, seconds: i64) {
    let at = if seconds >= 0 {
        SystemTime::now() - Duration::from_secs(seconds as u64)
    } else {
        SystemTime::now() + Duration::from_secs((-seconds) as u64)
    };
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}
