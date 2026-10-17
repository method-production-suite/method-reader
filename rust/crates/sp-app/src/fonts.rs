//! The window's fonts, in two kinds.
//!
//! The SCRIPT's faces: the same six WOFF2 subsets the website serves,
//! Courier Prime and Literata, embedded in the binary and decoded once at
//! start, so the window makes no network request and works offline. They
//! set the screenplay itself (the typed page, the scene index, the
//! Settings preview), where they mean "this is the script".
//!
//! The CHROME's face: every control, label, tab, heading and message is
//! the platform's own UI sans, as Method Budget's and Method Schedule's
//! windows are (their `--font-ui` is `-apple-system, "Segoe UI",
//! system-ui, ...`). egui cannot ask the system for a font by role, so
//! [`system_ui`] reads the face from where each platform keeps it (macOS:
//! San Francisco, /System/Library/Fonts/SFNS.ttf; Windows: Segoe UI from
//! the Fonts folder; Linux: the usual sans files where fontconfig's
//! packages install them: Cantarell, Noto Sans, Ubuntu, DejaVu Sans,
//! Liberation Sans). Nothing of Apple's or Microsoft's is bundled: the
//! face is read from the machine it runs on. Where none loads, Inter (release v4.1, font version 4.001)
//! (SIL Open Font License 1.1, unmodified upstream files in `fonts-ui/`)
//! stands in, and it follows the system face in every family for any
//! glyph the system face lacks.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use egui::epaint::text::{FontTweak, VariationCoords};
use egui::{FontData, FontDefinitions, FontFamily};

use crate::theme::{CHROME, CHROME_STRONG, PROSE, STRUCTURE, STRUCTURE_BOLD};

/// The bundled UI fallback: Inter's regular and semibold, unmodified.
pub const INTER_REGULAR: &[u8] = include_bytes!("../fonts-ui/Inter-Regular.ttf");
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../fonts-ui/Inter-SemiBold.ttf");

/// The chrome's two weights: text, and the family's `font-weight: 600`
/// for headings, the primary button and the selected tab.
pub const REGULAR_WEIGHT: f32 = 400.0;
pub const STRONG_WEIGHT: f32 = 600.0;

/// One system face as read from disk: its bytes (kept for the life of the
/// process: read once, shared by every window and test harness) and the
/// face index inside a collection.
#[derive(Clone, Copy, Debug)]
pub struct SystemFace {
    pub bytes: &'static [u8],
    pub index: u32,
    /// Where it came from, for the About sheet's honesty and the tests.
    pub path: &'static str,
}

/// The system's UI sans: a regular face and a strong one. On a variable
/// face (San Francisco) both are the same file at two weights.
#[derive(Clone, Copy, Debug)]
pub struct SystemUi {
    pub regular: SystemFace,
    pub strong: SystemFace,
}

/// Where each platform keeps its UI face, regular then strong. The first
/// pair whose regular file reads wins; a missing strong file falls back to
/// the regular one (at the strong weight when the face is variable).
fn candidates() -> Vec<(PathBuf, PathBuf)> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        let sf = PathBuf::from("/System/Library/Fonts/SFNS.ttf");
        out.push((sf.clone(), sf));
    } else if cfg!(windows) {
        let fonts = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
            .join("Fonts");
        out.push((fonts.join("segoeui.ttf"), fonts.join("seguisb.ttf")));
        out.push((fonts.join("segoeui.ttf"), fonts.join("segoeuib.ttf")));
    } else {
        for (regular, strong) in LINUX_FILES {
            out.push((PathBuf::from(regular), PathBuf::from(strong)));
        }
    }
    out
}

/// The usual Linux sans files, where the distributions' fontconfig
/// packages put them, in the order a desktop would prefer them. (No
/// `fc-match`: the window starts no child for its own drawing.)
const LINUX_FILES: [(&str, &str); 9] = [
    (
        "/usr/share/fonts/cantarell/Cantarell-VF.otf",
        "/usr/share/fonts/cantarell/Cantarell-VF.otf",
    ),
    (
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
    ),
    (
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-SemiBold.ttf",
    ),
    (
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSans-SemiBold.ttf",
    ),
    (
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-M.ttf",
    ),
    (
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ),
    (
        "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
        "/usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf",
    ),
];

fn read_face(path: &std::path::Path) -> Option<SystemFace> {
    let bytes = std::fs::read(path).ok()?;
    // A face egui cannot read is no face at all.
    let probe = FontData::from_owned(bytes);
    skrifa_ok(&probe)?;
    let bytes: &'static [u8] = Box::leak(probe.font.into_owned().into_boxed_slice());
    let path: &'static str = Box::leak(path.to_string_lossy().into_owned().into_boxed_str());
    Some(SystemFace {
        bytes,
        index: 0,
        path,
    })
}

/// Whether egui's font reader (skrifa) opens this face and maps the
/// letters a UI needs: egui panics on a face it cannot parse.
fn skrifa_ok(data: &FontData) -> Option<()> {
    use skrifa::MetadataProvider as _;
    let font = skrifa::FontRef::from_index(data.font.as_ref(), data.index).ok()?;
    let map = font.charmap();
    "Aa0".chars().all(|c| map.map(c).is_some()).then_some(())
}

/// The system's UI face, read once per process; None where none of the
/// candidates reads (then Inter stands in).
pub fn system_ui() -> Option<SystemUi> {
    static FACE: OnceLock<Option<SystemUi>> = OnceLock::new();
    *FACE.get_or_init(|| {
        if std::env::var_os("METHOD_READER_BUNDLED_UI_FONT").is_some() {
            return None;
        }
        for (regular, strong) in candidates() {
            let Some(r) = read_face(&regular) else {
                continue;
            };
            let s = if strong == regular {
                r
            } else {
                read_face(&strong).unwrap_or(r)
            };
            return Some(SystemUi {
                regular: r,
                strong: s,
            });
        }
        None
    })
}

/// A face at a weight: on a variable face with a weight axis the weight is
/// set (and San Francisco's optical size held at its text cut); a static
/// face is used as it is.
fn weighted(bytes: &'static [u8], index: u32, weight: f32) -> FontData {
    let mut data = FontData::from_static(bytes);
    data.index = index;
    let axes = data.variation_axes();
    let mut coords = VariationCoords::default();
    for axis in &axes {
        match axis.tag.to_string().as_str() {
            "wght" => coords.push(*b"wght", weight.clamp(axis.range.min, axis.range.max)),
            "opsz" => coords.push(*b"opsz", 13.0f32.clamp(axis.range.min, axis.range.max)),
            _ => {}
        }
    }
    if axes.is_empty() {
        return data;
    }
    data.tweak(FontTweak {
        coords,
        ..Default::default()
    })
}

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
    // The book's Sans typeface: egui's own proportional face, as before
    // (the book's look is the book's, not the chrome's).
    let sans = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    defs.families.insert(FontFamily::Name("sans".into()), sans);
    // The chrome: the system face first, Inter behind it, egui's own last.
    let egui_own = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    defs.font_data.insert(
        "inter-400".into(),
        Arc::new(weighted(INTER_REGULAR, 0, REGULAR_WEIGHT)),
    );
    defs.font_data.insert(
        "inter-600".into(),
        Arc::new(weighted(INTER_SEMIBOLD, 0, STRONG_WEIGHT)),
    );
    let mut ui_list = Vec::new();
    let mut strong_list = Vec::new();
    if let Some(face) = system_ui() {
        defs.font_data.insert(
            "system-ui-400".into(),
            Arc::new(weighted(
                face.regular.bytes,
                face.regular.index,
                REGULAR_WEIGHT,
            )),
        );
        defs.font_data.insert(
            "system-ui-600".into(),
            Arc::new(weighted(
                face.strong.bytes,
                face.strong.index,
                STRONG_WEIGHT,
            )),
        );
        ui_list.push("system-ui-400".to_string());
        strong_list.push("system-ui-600".to_string());
    }
    ui_list.push("inter-400".into());
    strong_list.push("inter-600".into());
    ui_list.extend(egui_own.iter().cloned());
    strong_list.extend(egui_own.iter().cloned());
    defs.families
        .insert(FontFamily::Name(CHROME.into()), ui_list.clone());
    defs.families
        .insert(FontFamily::Name(CHROME_STRONG.into()), strong_list);
    // egui's own widgets (tooltips, text fields, menus) speak the chrome's
    // face too; its monospace is the script's typewriter face.
    let mono = defs.families[&FontFamily::Name(STRUCTURE.into())].clone();
    defs.families.insert(FontFamily::Proportional, ui_list);
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
            defs.families[&FontFamily::Name(PROSE.into())][0],
            "literata-400-latin"
        );
    }

    // The chrome is the system's own sans where one reads, Inter otherwise,
    // and never the script's faces.
    #[test]
    fn the_chrome_is_the_system_sans_with_inter_behind_it() {
        let defs = definitions();
        let ui = &defs.families[&FontFamily::Name(CHROME.into())];
        let strong = &defs.families[&FontFamily::Name(CHROME_STRONG.into())];
        let first = if system_ui().is_some() {
            "system-ui-400"
        } else {
            "inter-400"
        };
        assert_eq!(ui[0], first);
        assert!(ui.contains(&"inter-400".to_string()));
        assert!(strong.contains(&"inter-600".to_string()));
        assert_eq!(&defs.families[&FontFamily::Proportional], ui);
        for list in [ui, strong] {
            assert!(
                !list
                    .iter()
                    .any(|f| f.starts_with("courier") || f.starts_with("literata")),
                "{list:?}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_a_mac_the_chrome_is_san_francisco_at_two_weights() {
        let face = system_ui().expect("SFNS.ttf reads on every supported macOS");
        assert!(
            face.regular.path.ends_with("SFNS.ttf"),
            "{}",
            face.regular.path
        );
        let data = weighted(face.regular.bytes, 0, STRONG_WEIGHT);
        assert!(
            data.tweak
                .coords
                .as_ref()
                .iter()
                .any(|(tag, v)| *tag == *b"wght" && *v == STRONG_WEIGHT),
            "{:?}",
            data.variation_axes()
        );
    }

    /// Inter, the fallback, is the unmodified upstream v4.1 release (font version 4.001), with
    /// its licence beside it.
    #[test]
    fn the_bundled_fallback_is_inter_unmodified() {
        assert_eq!(
            sha256(INTER_REGULAR),
            "40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82"
        );
        assert_eq!(
            sha256(INTER_SEMIBOLD),
            "78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3"
        );
        let licence =
            std::fs::read_to_string(format!("{}/fonts-ui/OFL.txt", env!("CARGO_MANIFEST_DIR")))
                .expect("fonts-ui/OFL.txt");
        assert!(licence.contains("SIL Open Font License"));
        assert!(licence.contains("The Inter Project Authors"));
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
