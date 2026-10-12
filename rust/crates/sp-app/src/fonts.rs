//! The bundled fonts: the same six WOFF2 subsets the website serves,
//! embedded in the binary and decoded once at start, so the window makes
//! no network request and works offline.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

use crate::theme::{PROSE, STRUCTURE, STRUCTURE_BOLD};

/// (name, WOFF2 bytes), latin before latin-ext so the subsets fall back in
/// the order the stylesheet's unicode-range gave them.
pub const FACES: [(&str, &[u8]); 6] = [
    (
        "courier-400-latin",
        include_bytes!("../fonts/courier-prime-400-latin.woff2"),
    ),
    (
        "courier-400-latin-ext",
        include_bytes!("../fonts/courier-prime-400-latin-ext.woff2"),
    ),
    (
        "courier-700-latin",
        include_bytes!("../fonts/courier-prime-700-latin.woff2"),
    ),
    (
        "courier-700-latin-ext",
        include_bytes!("../fonts/courier-prime-700-latin-ext.woff2"),
    ),
    (
        "literata-400-latin",
        include_bytes!("../fonts/literata-400-latin.woff2"),
    ),
    (
        "literata-400-latin-ext",
        include_bytes!("../fonts/literata-400-latin-ext.woff2"),
    ),
];

/// Every face decoded to the TrueType egui reads. A face that will not
/// decode is left out (egui's own fonts then stand in), never a panic.
pub fn definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let mut decoded = |name: &str, bytes: &[u8]| match wuff::decompress_woff2(bytes) {
        Ok(ttf) => {
            defs.font_data
                .insert(name.into(), Arc::new(FontData::from_owned(ttf)));
            true
        }
        Err(_) => false,
    };
    let ok: Vec<&str> = FACES
        .iter()
        .filter(|(n, b)| decoded(n, b))
        .map(|(n, _)| *n)
        .collect();
    let family = |defs: &mut FontDefinitions, family: &str, faces: &[&str]| {
        let mut list: Vec<String> = faces
            .iter()
            .filter(|f| ok.contains(f))
            .map(|f| f.to_string())
            .collect();
        // egui's own faces last, for anything ours do not cover.
        list.extend(
            defs.families
                .get(&FontFamily::Proportional)
                .cloned()
                .unwrap_or_default(),
        );
        defs.families.insert(FontFamily::Name(family.into()), list);
    };
    family(
        &mut defs,
        STRUCTURE,
        &["courier-400-latin", "courier-400-latin-ext"],
    );
    family(
        &mut defs,
        STRUCTURE_BOLD,
        &["courier-700-latin", "courier-700-latin-ext"],
    );
    family(
        &mut defs,
        PROSE,
        &["literata-400-latin", "literata-400-latin-ext"],
    );
    // The book's Sans typeface: egui's own proportional face.
    let sans = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    defs.families.insert(FontFamily::Name("sans".into()), sans);
    // The window's default body text is the prose face, and egui's own
    // monospace is the structure face, as the stylesheet set them.
    let prose = defs.families[&FontFamily::Name(PROSE.into())].clone();
    let mono = defs.families[&FontFamily::Name(STRUCTURE.into())].clone();
    defs.families.insert(FontFamily::Proportional, prose);
    defs.families.insert(FontFamily::Monospace, mono);
    defs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_face_decodes() {
        for (name, bytes) in FACES {
            let ttf = wuff::decompress_woff2(bytes).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert!(ttf.len() > bytes.len(), "{name} grew when decoded");
        }
        let defs = definitions();
        assert_eq!(
            defs.families[&FontFamily::Name(STRUCTURE.into())][0],
            "courier-400-latin"
        );
        assert_eq!(
            defs.families[&FontFamily::Name(STRUCTURE_BOLD.into())][1],
            "courier-700-latin-ext"
        );
        assert_eq!(
            defs.families[&FontFamily::Proportional][0],
            "literata-400-latin"
        );
    }

    /// The six files, pinned. They were desktop/ui/fonts' copies until the
    /// cutover deleted desktop/; sp-app's are now the window's own, so a
    /// changed byte is a decision made here, not drift.
    const SHA256: [(&str, &str); 6] = [
        (
            "courier-prime-400-latin",
            "21bf7ff8d9e067443a3fe0832ee8e4364faee9ace1fb0ec3e625cd09d231802f",
        ),
        (
            "courier-prime-400-latin-ext",
            "18c8e32ea1e572327d72c79493e5358187743f76c0ecbc58e0d3a51c57487741",
        ),
        (
            "courier-prime-700-latin",
            "06622415713b4150c18f4489db7c8850300d9f3e55051fc78ed33dca88c5ff08",
        ),
        (
            "courier-prime-700-latin-ext",
            "078c780980eb6dc4865a9d738b8230e0d3b5175b3ae7dee7ec4978d75af6efc3",
        ),
        (
            "literata-400-latin",
            "5b16e2fa223e5b6161bf4934df92bc1ca0b4f7b8f68df791a4d5104e392144d6",
        ),
        (
            "literata-400-latin-ext",
            "337ce4174059dbc01ef5b50d72ad0887b5ac3e2ea5622739fd9fc2da084034ba",
        ),
    ];

    fn file_of(face: &str) -> String {
        face.replace("courier-", "courier-prime-")
    }

    fn sha256(bytes: &[u8]) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn the_fonts_are_the_pinned_files() {
        assert_eq!(FACES.len(), SHA256.len());
        for ((name, bytes), (file, want)) in FACES.iter().zip(SHA256) {
            assert_eq!(file_of(name), file, "FACES and the pins are in one order");
            assert_eq!(&bytes[..4], b"wOF2", "{name} is a WOFF2");
            assert_eq!(sha256(bytes), want, "{name} is the pinned file");
        }
    }

    #[test]
    fn every_file_in_the_fonts_folder_is_bundled() {
        let dir = format!("{}/fonts", env!("CARGO_MANIFEST_DIR"));
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .expect("fonts/")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let mut bundled: Vec<String> = FACES
            .iter()
            .map(|(n, _)| format!("{}.woff2", file_of(n)))
            .collect();
        bundled.sort();
        assert_eq!(on_disk, bundled);
    }

    #[test]
    fn the_fonts_are_the_websites_own() {
        // The same six subsets site/fonts serves: the website stays after
        // the cutover (it is HTML, not app code).
        for (name, bytes) in FACES {
            let site = std::fs::read(format!(
                "{}/../../../site/fonts/{}.woff2",
                env!("CARGO_MANIFEST_DIR"),
                file_of(name)
            ))
            .expect(name);
            assert_eq!(site, bytes, "{name} is the same file the website serves");
        }
    }
}
