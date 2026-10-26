//! The `= pg N` page marker as a reader of the .fountain sees it: the one
//! definition sp-fdx asks (the EPUB and MOBI renderers carry the same
//! pattern, `/^pg\s+(\S+)$/` on the synopsis's trimmed text).

use sp_fountain::{page_marker_label, tokenize};
use sp_model::TokenType;

#[test]
fn a_marker_names_its_label() {
    assert_eq!(page_marker_label("pg 2"), Some("2"));
    assert_eq!(page_marker_label("  pg   12  "), Some("12"));
    assert_eq!(page_marker_label("pg\t7A"), Some("7A"));
}

#[test]
fn anything_else_is_an_ordinary_synopsis() {
    for text in ["pg", "pg ", "pg 2 3", "page 2", "pg2", "The setup", "PG 2"] {
        assert_eq!(page_marker_label(text), None, "{text:?}");
    }
}

#[test]
fn the_serializer_s_marker_reaches_it_as_a_synopsis() {
    let tokens = tokenize("INT. A - DAY\n\n= pg 4\n\nB.\n").unwrap();
    let synopsis = tokens
        .iter()
        .find(|t| t.kind == TokenType::Synopsis)
        .expect("a synopsis");
    assert_eq!(page_marker_label(synopsis.text_or_empty()), Some("4"));
}
