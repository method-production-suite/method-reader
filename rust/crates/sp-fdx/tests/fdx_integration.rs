//! The FDX end to end on the committed invented scripts, ported from
//! tests/fdx-integration.test.ts: checked as well-formed XML whose paragraph
//! sequence is exactly the one the fountain-js tokens call for. The expected
//! sequence is worked out here from the tokens with this file's OWN table,
//! not the renderer's, so a renderer that drops or invents a paragraph
//! cannot agree with itself.
//!
//! The TypeScript converts each PDF on the spot; until sp-parser and
//! sp-fountain land, the oracle's dump of that same conversion (its
//! script.fountain and tokens.json) stands in for it.

mod common;

use std::path::PathBuf;

use common::{XmlElement, parse_xml};
use sp_fdx::fountain_shim::{is_primary_slug, strip_notes};
use sp_fdx::tokens_to_fdx;
use sp_model::{Token, TokenType};

fn oracle(name: &str, file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle")
        .join(name)
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn tokens_of(name: &str) -> Vec<Token> {
    serde_json::from_str(&oracle(name, "tokens.json")).expect("tokens.json")
}

fn type_for(kind: &TokenType) -> Option<&'static str> {
    Some(match kind {
        TokenType::Action | TokenType::Centered => "Action",
        TokenType::Character => "Character",
        TokenType::Parenthetical => "Parenthetical",
        TokenType::Dialogue => "Dialogue",
        TokenType::Lyrics => "Lyrics",
        TokenType::Transition => "Transition",
        _ => return None,
    })
}

/// The paragraph sequence the tokens call for: one entry per non-blank line
/// of each content token, a dual exchange as `General[` ... `]`.
fn expected_sequence(tokens: &[Token]) -> Vec<String> {
    let mut out = Vec::new();
    for t in tokens {
        if t.is_title() {
            continue;
        }
        match t.kind {
            TokenType::DualDialogueBegin => {
                out.push("General[".to_owned());
                continue;
            }
            TokenType::DualDialogueEnd => {
                out.push("]".to_owned());
                continue;
            }
            _ => {}
        }
        let kind = if t.kind == TokenType::SceneHeading {
            Some(if is_primary_slug(t.text_or_empty()) {
                "Scene Heading"
            } else {
                "Shot"
            })
        } else {
            type_for(&t.kind)
        };
        let Some(kind) = kind else { continue };
        let stripped = strip_notes(t.text_or_empty());
        for _ in stripped.split('\n').filter(|l| !l.trim().is_empty()) {
            out.push(kind.to_owned());
        }
    }
    out
}

fn actual_sequence(content: &XmlElement) -> Vec<String> {
    let mut out = Vec::new();
    for p in content.elements(Some("Paragraph")) {
        let kind = p.attr("Type").unwrap_or("").to_owned();
        if let Some(dual) = p.child("DualDialogue") {
            out.push(format!("{kind}["));
            for q in dual.elements(Some("Paragraph")) {
                out.push(q.attr("Type").unwrap_or("").to_owned());
            }
            out.push("]".to_owned());
        } else {
            out.push(kind);
        }
    }
    out
}

fn all_paragraphs(el: &XmlElement) -> Vec<&XmlElement> {
    let mut found = Vec::new();
    for c in el.elements(None) {
        if c.name == "Paragraph" {
            found.push(c);
        }
        found.extend(all_paragraphs(c));
    }
    found
}

const SCRIPTS: [&str; 3] = ["field-station", "screenplay", "torture"];

#[test]
fn is_well_formed_xml_with_a_finaldraft_root() {
    for name in SCRIPTS {
        let root = parse_xml(&tokens_to_fdx(&tokens_of(name)));
        assert_eq!(root.name, "FinalDraft", "{name}");
        assert_eq!(root.attr("DocumentType"), Some("Script"), "{name}");
    }
}

#[test]
fn its_paragraph_types_follow_the_fountain_js_token_sequence_exactly() {
    for name in SCRIPTS {
        let tokens = tokens_of(name);
        let root = parse_xml(&tokens_to_fdx(&tokens));
        let expected = expected_sequence(&tokens);
        assert!(expected.len() > 20, "{name}");
        assert_eq!(
            actual_sequence(root.child("Content").unwrap()),
            expected,
            "{name}"
        );
    }
}

#[test]
fn carries_a_title_page_with_the_scripts_title_on_it() {
    for name in SCRIPTS {
        let tokens = tokens_of(name);
        let title = tokens
            .iter()
            .find(|t| t.kind == TokenType::Other("title".to_owned()))
            .and_then(|t| t.text.clone())
            .unwrap_or_default();
        assert!(!title.is_empty(), "{name}");
        let root = parse_xml(&tokens_to_fdx(&tokens));
        let tp = root.child("TitlePage").unwrap().child("Content").unwrap();
        let texts: Vec<String> = tp
            .elements(Some("Paragraph"))
            .iter()
            .map(|p| p.text())
            .collect();
        // Emphasis markers become styles, so compare without them.
        let first: String = title
            .split('\n')
            .next()
            .unwrap()
            .chars()
            .filter(|c| !matches!(c, '*' | '_'))
            .collect();
        assert!(texts.contains(&first), "{name}");
    }
}

#[test]
fn no_note_sigil_or_page_marker_leaks_into_any_paragraph() {
    for name in SCRIPTS {
        let fdx = tokens_to_fdx(&tokens_of(name));
        assert!(!fdx.contains("[["), "{name}");
        assert!(!fdx.contains("]]"), "{name}");
        let root = parse_xml(&fdx);
        for p in all_paragraphs(&root) {
            if p.child("DualDialogue").is_some() {
                continue;
            }
            let text = p.text();
            match p.attr("Type") {
                Some("Character") => {
                    assert!(
                        !text.starts_with('@') && !text.trim_end().ends_with('^'),
                        "{name}"
                    );
                }
                Some("Shot") | Some("Scene Heading") => assert!(!text.starts_with('.'), "{name}"),
                Some("Transition") => assert!(!text.starts_with('>'), "{name}"),
                Some("Action") => {
                    assert!(
                        !text.starts_with('!') && !text.starts_with("= pg "),
                        "{name}"
                    );
                }
                _ => {}
            }
        }
    }
}

#[test]
fn every_paragraph_holds_at_least_one_text_run_and_no_stray_text_between_runs() {
    for name in SCRIPTS {
        let root = parse_xml(&tokens_to_fdx(&tokens_of(name)));
        for p in all_paragraphs(&root) {
            if p.child("DualDialogue").is_some() {
                assert_eq!(p.children.len(), 1, "{name}");
                continue;
            }
            assert!(!p.elements(Some("Text")).is_empty(), "{name}");
            assert!(!p.has_text_children(), "{name}");
        }
    }
}

/// The committed picture fixture: its .fountain names pictures in
/// [[img: ...]] notes, and its FDX names none of them.
#[test]
fn images_pdf_records_its_pictures_and_the_fdx_carries_no_trace_of_them() {
    assert!(oracle("images", "script.fountain").contains("[[img: "));
    let fdx = tokens_to_fdx(&tokens_of("images"));
    assert_eq!(parse_xml(&fdx).name, "FinalDraft");
    assert!(!fdx.contains("img:"));
    assert!(!fdx.contains(".png"));
    assert!(!fdx.contains("Illustration"));
}
