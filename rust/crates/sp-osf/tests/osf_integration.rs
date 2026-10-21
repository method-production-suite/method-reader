//! The OSF end to end on the committed invented scripts' tokens (the
//! oracle's tokens.json for each fixture, the same input sp-fdx's
//! integration test reads): well-formed XML whose paragraph sequence is
//! exactly the one the tokens call for. The expected sequence is worked out
//! here from the tokens with this file's OWN table, not the writer's, so a
//! writer that drops or invents a paragraph cannot agree with itself.
//! sp-engine's tests/osf.rs converts the PDFs themselves.

mod common;

use std::path::PathBuf;

use common::{Para, paras_of, parse_xml};
use sp_fountain::{is_primary_slug, strip_notes};
use sp_model::{Token, TokenType};
use sp_osf::tokens_to_osf;

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

/// The paragraph sequence the tokens call for: one entry per content token
/// with any text left after its notes, a lyric as dialogue inside a speech
/// and action outside one, and a `*` after the first cue of a dual pair.
fn expected_sequence(tokens: &[Token]) -> Vec<String> {
    let mut out = Vec::new();
    let (mut in_speech, mut dual) = (false, false);
    for t in tokens {
        if t.is_title() {
            continue;
        }
        let kind = match &t.kind {
            TokenType::DialogueBegin => {
                in_speech = true;
                continue;
            }
            TokenType::DialogueEnd => {
                in_speech = false;
                continue;
            }
            TokenType::DualDialogueBegin => {
                dual = true;
                continue;
            }
            TokenType::DualDialogueEnd => {
                dual = false;
                continue;
            }
            TokenType::SceneHeading if is_primary_slug(t.text_or_empty()) => "Scene Heading",
            TokenType::SceneHeading => "Shot",
            TokenType::Action | TokenType::Centered => "Action",
            TokenType::Character => "Character",
            TokenType::Parenthetical => "Parenthetical",
            TokenType::Dialogue => "Dialogue",
            TokenType::Lyrics if in_speech => "Dialogue",
            TokenType::Lyrics => "Action",
            TokenType::Transition => "Transition",
            _ => continue,
        };
        if strip_notes(t.text_or_empty()).trim().is_empty() {
            continue;
        }
        if dual && kind == "Character" {
            dual = false;
            out.push("Character*".to_owned());
        } else {
            out.push(kind.to_owned());
        }
    }
    out
}

fn actual_sequence(paras: &[Para]) -> Vec<String> {
    paras
        .iter()
        .map(|p| {
            let mark = if p.style.attr("dualdialogue") == Some("1") {
                "*"
            } else {
                ""
            };
            format!("{}{mark}", p.kind())
        })
        .collect()
}

const SCRIPTS: [&str; 3] = ["field-station", "screenplay", "torture"];

#[test]
fn is_well_formed_xml_with_an_osf_document_root() {
    for name in SCRIPTS {
        let root = parse_xml(&tokens_to_osf(&tokens_of(name)));
        assert_eq!(root.name, "document", "{name}");
        assert_eq!(
            root.attr("type"),
            Some("Open Screenplay Format document"),
            "{name}"
        );
        assert_eq!(root.attr("version"), Some(sp_osf::OSF_VERSION), "{name}");
    }
}

#[test]
fn its_paragraph_styles_follow_the_token_sequence_exactly() {
    for name in SCRIPTS {
        let tokens = tokens_of(name);
        let root = parse_xml(&tokens_to_osf(&tokens));
        let expected = expected_sequence(&tokens);
        assert!(expected.len() > 20, "{name}");
        assert_eq!(
            actual_sequence(&paras_of(&root, "paragraphs").unwrap()),
            expected,
            "{name}"
        );
    }
}

#[test]
fn torture_marks_one_cue_per_dual_exchange() {
    let torture = parse_xml(&tokens_to_osf(&tokens_of("torture")));
    let marked = paras_of(&torture, "paragraphs")
        .unwrap()
        .iter()
        .filter(|p| p.style.attr("dualdialogue").is_some())
        .count();
    let exchanges = tokens_of("torture")
        .iter()
        .filter(|t| t.kind == TokenType::DualDialogueBegin)
        .count();
    assert!(exchanges >= 2, "torture.pdf's sheet 9 has two");
    assert_eq!(marked, exchanges);
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
        let root = parse_xml(&tokens_to_osf(&tokens));
        let texts: Vec<String> = paras_of(&root, "titlepage")
            .expect("a titlepage")
            .iter()
            .map(|p| p.text.clone())
            .collect();
        let first: String = title
            .split('\n')
            .next()
            .unwrap()
            .chars()
            .filter(|c| !matches!(c, '*' | '_'))
            .collect();
        assert!(texts.contains(&first.trim().to_owned()), "{name}");
    }
}

#[test]
fn no_note_sigil_or_page_marker_leaks_into_any_paragraph() {
    for name in SCRIPTS {
        let osf = tokens_to_osf(&tokens_of(name));
        assert!(!osf.contains("[["), "{name}");
        assert!(!osf.contains("]]"), "{name}");
        let root = parse_xml(&osf);
        for p in paras_of(&root, "paragraphs").unwrap() {
            let text = p.text.as_str();
            assert!(!text.is_empty(), "{name}: an empty paragraph");
            match p.kind() {
                "Character" => assert!(
                    !text.starts_with('@') && !text.trim_end().ends_with('^'),
                    "{name}"
                ),
                "Shot" | "Scene Heading" => assert!(!text.starts_with('.'), "{name}"),
                "Transition" => assert!(!text.starts_with('>'), "{name}"),
                "Action" => assert!(
                    !text.starts_with('!') && !text.starts_with("= pg "),
                    "{name}"
                ),
                _ => {}
            }
        }
    }
}

#[test]
fn images_pdf_records_its_pictures_and_the_osf_carries_no_trace_of_them() {
    assert!(oracle("images", "script.fountain").contains("[[img: "));
    let osf = tokens_to_osf(&tokens_of("images"));
    assert_eq!(parse_xml(&osf).name, "document");
    assert!(!osf.contains("img:"));
    assert!(!osf.contains(".png"));
}
