//! The four `[[fmt]]` note checks of tests/torture.test.ts ("block font
//! shifts"): note PLACEMENT is a serializer fact, so they read the
//! .fountain made from torture.pdf. As in the TypeScript, the text is
//! parsed alone (`parse_lines(lines, &[])`, no pictures) and serialized
//! with the default options. They sit here rather than in sp-fountain
//! because they need the PDF read (sp-pdf) and parsed (sp-parser); the rest
//! of the file is sp-parser/tests/torture.rs.

mod common;

use std::sync::OnceLock;

use sp_fountain::to_fountain;
use sp_model::DEFAULT_FORMAT_OPTIONS;
use sp_parser::extract::extract_document;
use sp_parser::parse_lines;

fn fountain() -> &'static str {
    static FOUNTAIN: OnceLock<String> = OnceLock::new();
    FOUNTAIN.get_or_init(|| {
        let pdf = std::fs::read(common::repo_root().join("tests/fixtures/torture.pdf"))
            .expect("torture.pdf");
        let facts = sp_pdf::read(pdf).expect("a PDF");
        let lines = extract_document(&facts, None, None).lines;
        to_fountain(&parse_lines(&lines, &[]), None, &DEFAULT_FORMAT_OPTIONS)
    })
}

#[test]
fn a_family_shift_becomes_a_note_glued_above_its_action_block() {
    assert!(fountain().contains("[[fmt: sans]]\nCHYRON: EVERY NAME HERE IS INVENTED."));
}

#[test]
fn all_three_size_steps_appear_each_on_its_own_block() {
    let f = fountain();
    assert!(f.contains("[[fmt: -1]]\nA footnote, set smaller than the body."));
    assert!(f.contains("[[fmt: +1]]\nThe index card, enlarged for the reader."));
    assert!(f.contains("[[fmt: +2]]\nA SIGN, VERY LARGE INDEED"));
}

#[test]
fn a_dialogue_shift_leads_the_line_instead() {
    assert!(fountain().contains("[[fmt: sans]] A text message, in the phone"));
}

#[test]
fn the_body_itself_carries_no_notes() {
    // A uniform Courier-12 screenplay must produce zero fmt by construction;
    // only the five deliberately-shifted blocks may have one.
    assert_eq!(fountain().matches("[[fmt:").count(), 5);
}
