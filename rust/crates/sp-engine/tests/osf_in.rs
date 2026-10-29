//! OSF in: an .osf someone chose is read into a `.fountain` text
//! (sp-osf's reader) and then converts exactly as a chosen `.fountain`
//! does, guards and all (docs/osf-input.md). The OSF here is Method
//! Reader's own, written from the invented demo, tests/fixtures/
//! field-station.pdf, or hand-written with invented text.

mod common;

use common::epub::{body_of, toc_of};
use sp_engine::convert::{
    ConvertError, ConvertOptions, convert_chosen_fountain, convert_pdf, osf_fountain_text,
};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(common::repo_root().join("tests/fixtures").join(name)).expect(name)
}

fn osf(body: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<document type=\"Open Screenplay Format document\" version=\"30\">\n<paragraphs>\n{body}\n</paragraphs>\n</document>\n"
    )
    .into_bytes()
}

#[test]
fn field_station_as_osf_makes_the_same_book_contents() {
    let from_pdf =
        convert_pdf(fixture("field-station.pdf"), ConvertOptions::default()).expect("converts");
    let tokens = sp_fountain::tokenize(&from_pdf.fountain_text).expect("tokens");
    let document = sp_osf::tokens_to_osf(&tokens);

    let text = osf_fountain_text(document.into_bytes()).expect("reads");
    let from_osf =
        convert_chosen_fountain(&text, ConvertOptions::default(), None).expect("converts");
    assert_eq!(from_osf.title, from_pdf.title);
    assert_eq!(from_osf.author, from_pdf.author);
    assert!(from_osf.warnings.is_empty(), "{:?}", from_osf.warnings);
    assert_eq!(toc_of(&from_osf.epub), toc_of(&from_pdf.epub));
    // The book's source is the .fountain the reader wrote.
    assert_eq!(from_osf.fountain_text, text);
    let body = body_of(&from_osf.epub);
    assert!(body.contains("Thirty-one."), "{body}");
}

#[test]
fn bytes_that_are_not_utf8_are_refused_as_a_fountain_is() {
    let mut bytes = osf("<para><style basestylename=\"Action\"/><text>CAF</text></para>");
    let at = bytes.iter().position(|&b| b == b'F').unwrap() + 1;
    bytes.insert(at, 0xc9);
    assert_eq!(osf_fountain_text(bytes), Err(ConvertError::NotUtf8));
}

#[test]
fn text_that_is_not_xml_is_refused_in_plain_words() {
    let err = osf_fountain_text(b"INT. HUT - NIGHT\n\nRain.\n".to_vec()).expect_err("refused");
    assert!(matches!(err, ConvertError::NotOsf(_)), "{err:?}");
    assert!(
        err.to_string()
            .starts_with("this script is not a readable OSF file: it is not well-formed XML"),
        "{err}"
    );
}

#[test]
fn xml_with_no_paragraphs_is_refused_in_plain_words() {
    let err = osf_fountain_text(b"<document version=\"30\"/>".to_vec()).expect_err("refused");
    assert_eq!(
        err.to_string(),
        "this file is XML but not an OSF script: it has no <paragraphs>"
    );
}

#[test]
fn an_osf_with_no_paragraphs_in_it_is_an_empty_script() {
    let text = osf_fountain_text(osf("")).expect("reads");
    let err = convert_chosen_fountain(
        &text,
        ConvertOptions {
            force: true,
            ..ConvertOptions::default()
        },
        None,
    )
    .expect_err("refused");
    assert_eq!(err, ConvertError::EmptyScript);
}

#[test]
fn an_osf_of_only_prose_is_not_a_screenplay_unless_forced() {
    let text = osf_fountain_text(osf(
        "<para><style basestylename=\"Action\"/><text>Bread, tea, batteries.</text></para>",
    ))
    .expect("reads");
    let err = convert_chosen_fountain(&text, ConvertOptions::default(), None).expect_err("refused");
    assert_eq!(err, ConvertError::NotAScreenplay);
    let forced = convert_chosen_fountain(
        &text,
        ConvertOptions {
            force: true,
            ..ConvertOptions::default()
        },
        None,
    )
    .expect("converts");
    assert!(body_of(&forced.epub).contains("Bread, tea, batteries."));
}
