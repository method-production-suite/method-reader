//! The OSF from a real conversion: field-station.pdf and torture.pdf go
//! through `convert_pdf`, and the .fountain it writes goes through the
//! export path's renderer (`Books::fountain_to_osf`), exactly what
//! `method-reader export --for osf` writes. Each document must parse as
//! well-formed XML and hold the script's structure: the scenes, the cues,
//! the dual exchanges, the title page, and none of Method Reader's own
//! notes. sp-osf's tests/osf.rs holds the element mapping case by case.

mod common;

use std::sync::OnceLock;

use sp_engine::convert::{ConvertOptions, convert_pdf};
use sp_engine::export::renderers::{Books, Renderers};

struct Converted {
    fountain: String,
    osf: String,
}

fn convert(name: &str) -> Converted {
    let pdf = std::fs::read(common::repo_root().join(format!("tests/fixtures/{name}.pdf")))
        .unwrap_or_else(|e| panic!("{name}.pdf: {e}"));
    let fountain = convert_pdf(pdf, ConvertOptions::default())
        .expect("converts")
        .fountain_text;
    let osf = Books.fountain_to_osf(&fountain).expect("an OSF");
    Converted { fountain, osf }
}

fn field_station() -> &'static Converted {
    static ONE: OnceLock<Converted> = OnceLock::new();
    ONE.get_or_init(|| convert("field-station"))
}

fn torture() -> &'static Converted {
    static ONE: OnceLock<Converted> = OnceLock::new();
    ONE.get_or_init(|| convert("torture"))
}

fn both() -> [(&'static str, &'static Converted); 2] {
    [("field-station", field_station()), ("torture", torture())]
}

/// One `<para>`: (its style, its style element's attributes, its text).
type Para = (String, Vec<(String, String)>, String);

/// Each `<para>` of one block.
fn paras(doc: &roxmltree::Document<'_>, block: &str) -> Vec<Para> {
    let Some(b) = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name(block))
    else {
        return Vec::new();
    };
    b.children()
        .filter(|n| n.has_tag_name("para"))
        .map(|p| {
            let style = p
                .children()
                .find(|n| n.has_tag_name("style"))
                .expect("every para has a style");
            let text: String = p
                .children()
                .filter(|n| n.has_tag_name("text"))
                .map(|t| t.text().unwrap_or(""))
                .collect();
            (
                style.attribute("basestylename").unwrap_or("").to_owned(),
                style
                    .attributes()
                    .map(|a| (a.name().to_owned(), a.value().to_owned()))
                    .collect(),
                text,
            )
        })
        .collect()
}

fn count(p: &[Para], style: &str) -> usize {
    p.iter().filter(|x| x.0 == style).count()
}

#[test]
fn both_scripts_are_well_formed_osf_documents() {
    for (name, c) in both() {
        let doc = roxmltree::Document::parse(&c.osf)
            .unwrap_or_else(|e| panic!("{name}: not well-formed XML: {e}"));
        let root = doc.root_element();
        assert_eq!(root.tag_name().name(), "document", "{name}");
        assert_eq!(
            root.attribute("type"),
            Some("Open Screenplay Format document"),
            "{name}"
        );
        assert_eq!(
            root.attribute("version"),
            Some(sp_osf::OSF_VERSION),
            "{name}"
        );
        let blocks: Vec<&str> = root
            .children()
            .filter(|n| n.is_element())
            .map(|n| n.tag_name().name())
            .collect();
        assert_eq!(blocks, ["paragraphs", "titlepage"], "{name}");
    }
}

#[test]
fn every_scene_heading_and_cue_of_the_fountain_is_a_paragraph() {
    for (name, c) in both() {
        let tokens = sp_fountain::tokenize(&c.fountain).unwrap();
        let doc = roxmltree::Document::parse(&c.osf).unwrap();
        let body = paras(&doc, "paragraphs");
        let headings = tokens
            .iter()
            .filter(|t| t.kind == sp_model::TokenType::SceneHeading)
            .count();
        let cues = tokens
            .iter()
            .filter(|t| t.kind == sp_model::TokenType::Character)
            .count();
        assert!(headings >= 5, "{name}: {headings} scene headings");
        assert_eq!(
            count(&body, "Scene Heading") + count(&body, "Shot"),
            headings,
            "{name}"
        );
        assert_eq!(count(&body, "Character"), cues, "{name}");
        assert!(count(&body, "Dialogue") >= cues, "{name}");
        assert!(count(&body, "Action") > 0, "{name}");
    }
}

#[test]
fn torture_carries_its_two_dual_exchanges_and_field_station_its_title() {
    let doc = roxmltree::Document::parse(&torture().osf).unwrap();
    let marked: Vec<String> = paras(&doc, "paragraphs")
        .into_iter()
        .filter(|p| p.1.iter().any(|(k, v)| k == "dualdialogue" && v == "1"))
        .map(|p| {
            assert_eq!(p.0, "Character");
            p.2
        })
        .collect();
    assert_eq!(
        marked.len(),
        2,
        "torture.pdf's two dual exchanges: {marked:?}"
    );

    let doc = roxmltree::Document::parse(&field_station().osf).unwrap();
    let title = paras(&doc, "titlepage");
    let centered: Vec<&str> = title
        .iter()
        .filter(|p| p.1.iter().any(|(k, v)| k == "align" && v == "center") && !p.2.is_empty())
        .map(|p| p.2.as_str())
        .collect();
    assert!(
        centered
            .iter()
            .any(|t| t.eq_ignore_ascii_case("field station")),
        "{centered:?}"
    );
    assert!(title.iter().all(|p| p.0 == "Normal Text"));
}

#[test]
fn none_of_method_readers_own_notes_reach_it() {
    for (name, c) in both() {
        assert!(!c.osf.contains("[["), "{name}");
        assert!(!c.osf.contains("fmt:"), "{name}");
        assert!(!c.osf.contains("img:"), "{name}");
    }
    assert!(
        torture().fountain.contains("[[fmt: "),
        "torture.pdf's .fountain carries [[fmt]] notes to strip"
    );
}

#[test]
fn the_page_breaks_of_the_fountain_are_page_breaks_in_it() {
    for (name, c) in both() {
        let tokens = sp_fountain::tokenize(&c.fountain).unwrap();
        let breaks = tokens
            .iter()
            .filter(|t| t.kind == sp_model::TokenType::PageBreak)
            .count();
        let doc = roxmltree::Document::parse(&c.osf).unwrap();
        let marked = paras(&doc, "paragraphs")
            .iter()
            .filter(|p| p.1.iter().any(|(k, _)| k == "pagebreakbefore"))
            .count();
        // A break at the very end has no paragraph to land on.
        assert!(
            marked <= breaks && marked + 1 >= breaks,
            "{name}: {marked} of {breaks}"
        );
    }
}
