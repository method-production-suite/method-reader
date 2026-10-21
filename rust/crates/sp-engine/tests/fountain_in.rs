//! Fountain in: a `.fountain` someone chose becomes the same books a PDF's
//! `.fountain` does, after the guards a text file can be held to
//! (docs/fountain-input.md, "The guards").
//!
//! The fixture is tests/fixtures/weather-log.fountain, an invented script
//! written for this test. The round trip runs the invented demo,
//! tests/fixtures/field-station.pdf, to its `.fountain` and back.

mod common;

use std::collections::BTreeMap;

use common::epub::{body_of, image_entries, opf, read_zip, toc_of};
use sp_engine::convert::{
    ConvertError, ConvertOptions, convert_chosen_fountain, convert_pdf, fountain_guards,
    fountain_text,
};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(common::repo_root().join("tests/fixtures").join(name)).expect(name)
}

fn chosen(text: &str) -> Result<sp_engine::convert::ConvertResult, ConvertError> {
    convert_chosen_fountain(text, ConvertOptions::default(), None)
}

#[test]
fn the_invented_fountain_converts_end_to_end() {
    let text = fountain_text(fixture("weather-log.fountain")).expect("UTF-8");
    let result = convert_chosen_fountain(
        &text,
        ConvertOptions {
            mobi: true,
            ..ConvertOptions::default()
        },
        None,
    )
    .expect("converts");
    assert_eq!(result.title, "Weather Log");
    assert_eq!(result.author.as_deref(), Some("the Method Reader project"));
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    // The text is the book's source exactly as it was chosen.
    assert_eq!(result.fountain_text, text);
    assert!(result.screenplay.is_none());
    assert!(result.images.is_empty());

    // The EPUB, read back the way every conversion test reads one.
    let entries = read_zip(&result.epub);
    assert_eq!(entries[0].name, "mimetype");
    assert_eq!(entries[0].data, b"application/epub+zip");
    assert!(entries.iter().any(|e| e.name == "META-INF/container.xml"));
    let package = opf(&result.epub);
    assert!(
        package.contains("<dc:title>Weather Log</dc:title>"),
        "{package}"
    );
    assert!(package.contains("<dc:creator"), "{package}");
    let toc: Vec<String> = toc_of(&result.epub)
        .into_iter()
        .map(|(_, title)| title)
        .collect();
    assert_eq!(
        toc,
        [
            "INT. RECORDING HUT - NIGHT",
            "EXT. MAST - CONTINUOUS",
            "INT. RECORDING HUT - LATER"
        ],
        "one contents entry per scene heading, in order"
    );
    let body = body_of(&result.epub);
    for line in [
        "Nine ninety-one. Falling.",
        "(not looking up)",
        "We log it, we send it, and we wait.",
        "CUT TO:",
        "THE END",
    ] {
        assert!(body.contains(line), "the book lost {line:?}");
    }
    assert!(body.contains("<em>slowly</em>"), "emphasis carries over");
    assert!(
        !body.contains("invisible in the book"),
        "a note never reaches the book"
    );
    assert!(image_entries(&result.epub).is_empty());
    assert!(
        result.mobi.is_some(),
        "the .mobi comes from the same tokens"
    );
    assert!(result.preview_html.contains("Nine ninety-one."));
}

// ---- the guards ----

#[test]
fn bytes_that_are_not_utf8_are_refused_in_plain_words() {
    // "CAFÉ" in Latin-1: the É is one byte UTF-8 cannot start with.
    let err = fountain_text(b"INT. CAF\xc9 - DAY\n".to_vec()).unwrap_err();
    assert_eq!(err, ConvertError::NotUtf8);
    assert_eq!(
        err.to_string(),
        "this script is not UTF-8 text. Save it as UTF-8 plain text and try again"
    );
    // A UTF-8 byte order mark is UTF-8, and is kept.
    assert_eq!(
        fountain_text(b"\xef\xbb\xbfINT. A - DAY".to_vec()).unwrap(),
        "\u{feff}INT. A - DAY"
    );
}

#[test]
fn an_empty_script_is_refused_even_with_force() {
    for text in ["", "\n\n", "  \t\r\n", "\u{feff}\n"] {
        assert_eq!(fountain_guards(text, false), Err(ConvertError::EmptyScript));
        assert_eq!(fountain_guards(text, true), Err(ConvertError::EmptyScript));
    }
    assert_eq!(
        ConvertError::EmptyScript.to_string(),
        "this script is empty: there is nothing in it to make a book from"
    );
}

#[test]
fn no_scene_heading_and_no_dialogue_is_not_a_screenplay_unless_forced() {
    let notes = "Shopping list\n\nBread, tea, batteries.\n\nCall the boatyard about the mooring.\n";
    assert_eq!(chosen(notes).unwrap_err(), ConvertError::NotAScreenplay);
    // A title page alone is no script either.
    assert_eq!(
        chosen("Title: Only A Title\nAuthor: Nobody\n").unwrap_err(),
        ConvertError::NotAScreenplay
    );
    let forced = convert_chosen_fountain(
        notes,
        ConvertOptions {
            force: true,
            ..ConvertOptions::default()
        },
        None,
    )
    .expect("--force overrules it, as it does for a PDF");
    assert!(body_of(&forced.epub).contains("batteries"));
    assert_eq!(
        forced.warnings,
        ["No scene headings detected: the EPUB will have no scene navigation."]
    );
}

#[test]
fn dialogue_without_a_scene_heading_converts_with_the_pdfs_warning() {
    let result = chosen("ODA\nNine ninety-one.\n").expect("dialogue is enough");
    assert_eq!(
        result.warnings,
        ["No scene headings detected: the EPUB will have no scene navigation."]
    );
    assert!(body_of(&result.epub).contains("Nine ninety-one."));
}

#[test]
fn a_scene_heading_without_dialogue_converts_silently() {
    let result = chosen("INT. RECORDING HUT - NIGHT\n\nThe kettle boils.\n").expect("converts");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
}

#[test]
fn the_guards_add_their_warning_ahead_of_the_fountains_own() {
    let result = convert_chosen_fountain(
        "@ODA (CONT'D)\nStill falling.\n",
        ConvertOptions {
            format: sp_model::FormatOptions {
                contd_mode: sp_model::ContdMode::Strip,
                ..sp_model::DEFAULT_FORMAT_OPTIONS
            },
            ..ConvertOptions::default()
        },
        None,
    )
    .expect("converts");
    assert_eq!(result.warnings.len(), 2, "{:?}", result.warnings);
    assert!(result.warnings[0].starts_with("No scene headings detected"));
    assert!(result.warnings[1].starts_with("This book still shows (CONT'D)"));
}

// ---- the round trip ----

/// tests/fixtures/field-station.pdf to its `.fountain` (and the pictures a
/// PDF conversion writes beside it), then that `.fountain` converted as a
/// chosen script: the same EPUB, byte for byte. The book's identity is its
/// `.fountain`'s content, its title and author ride in that `.fountain`'s
/// title page, and both paths end in the same renderer, so nothing is left
/// to differ.
#[test]
fn field_station_pdf_to_fountain_and_back_gives_the_same_epub_bytes() {
    let from_pdf =
        convert_pdf(fixture("field-station.pdf"), ConvertOptions::default()).expect("converts");
    let pictures: BTreeMap<String, Vec<u8>> = from_pdf
        .images
        .iter()
        .map(|i| (i.name.clone(), i.bytes.clone()))
        .collect();
    let text = fountain_text(from_pdf.fountain_text.clone().into_bytes()).expect("UTF-8");
    let from_fountain = convert_chosen_fountain(&text, ConvertOptions::default(), Some(&pictures))
        .expect("converts");
    assert_eq!(from_fountain.title, from_pdf.title);
    assert_eq!(from_fountain.author, from_pdf.author);
    assert!(
        from_fountain.warnings.is_empty(),
        "{:?}",
        from_fountain.warnings
    );
    assert!(
        from_fountain.epub == from_pdf.epub,
        "the round trip changed the EPUB"
    );
    assert_eq!(from_fountain.preview_html, from_pdf.preview_html);
}
