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

    #[test]
    fn the_fonts_are_the_windows_own() {
        for (name, bytes) in FACES {
            let file = name.replace("courier-", "courier-prime-");
            let ui = std::fs::read(format!(
                "{}/../../../desktop/ui/fonts/{file}.woff2",
                env!("CARGO_MANIFEST_DIR")
            ))
            .expect(name);
            assert_eq!(ui, bytes, "{name} is the same file the window ships");
        }
    }
}
