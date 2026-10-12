//! The KF8 markup and the chunker (ports tests/azw3-chunk.test.ts).

mod common;

use std::collections::HashMap;

use common::fixture_epub;
use sp_azw3::chunk::{CHUNK_SIZE, Kf8Markup, ResourceRef, kf8_markup};
use sp_azw3::epub::{EpubBook, SpineDoc, read_epub};
use sp_azw3::xml::{XDocument, parse_xml};

/// Rebuild each file from flow 0 and the two tables, as a reader does.
fn rebuild_files(m: &Kf8Markup) -> Vec<String> {
    let text = &m.flows[0];
    let mut c = 0;
    m.skel_table
        .iter()
        .map(|s| {
            let mut file = text[s.start_pos..s.start_pos + s.length].to_vec();
            let mut base = s.start_pos + s.length;
            for _ in 0..s.chunk_count {
                let chunk = &m.chunk_table[c];
                let part = &text[base..base + chunk.length];
                let at = chunk.insert_pos - s.start_pos;
                file.splice(at..at, part.iter().copied());
                base += chunk.length;
                c += 1;
            }
            String::from_utf8(file).unwrap()
        })
        .collect()
}

fn book_of(bodies: &[&str]) -> EpubBook {
    EpubBook {
        title: "T".into(),
        authors: vec![],
        language: "en".into(),
        identifier: None,
        modified: None,
        spine: bodies
            .iter()
            .enumerate()
            .map(|(i, b)| SpineDoc {
                href: format!("text/f{i}.xhtml"),
                doc: parse_xml(&format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n<head>\n<title>T</title>\n<link rel=\"stylesheet\" type=\"text/css\" href=\"../style.css\"/>\n</head>\n<body>\n{b}</body>\n</html>\n"
                ))
                .unwrap(),
            })
            .collect(),
        stylesheets: vec![("style.css".into(), "p { margin: 0 }".into())],
        images: vec![],
        cover_href: None,
        toc: vec![],
        landmarks: vec![],
    }
}

fn norm(s: &str) -> String {
    s.split(sp_fountain::js::is_space)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn text_of(doc: &XDocument) -> String {
    norm(&doc.root.text_content())
}

fn check_markup(mut book: EpubBook) -> (Kf8Markup, Vec<String>) {
    let original: Vec<String> = book.spine.iter().map(|s| text_of(&s.doc)).collect();
    let m = kf8_markup(&mut book, &HashMap::new()).unwrap();
    let files = rebuild_files(&m);
    assert_eq!(files.len(), original.len());
    for (i, f) in files.iter().enumerate() {
        // Every fragment insert lands between tags, and the file is whole XML
        // with the same text as the document it came from.
        let doc = parse_xml(f).unwrap();
        assert_eq!(text_of(&doc), original[i]);
        assert!(f.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\">"));
        assert!(f.contains("href=\"kindle:flow:0001?mime=text/css\""));
    }
    for c in &m.chunk_table {
        assert!(c.length <= CHUNK_SIZE + 64);
    }
    // Every aid's offset is the start of the tag that carries it.
    let rebuilt = files.concat().into_bytes();
    for (aid, &(_, _, offset)) in &m.aid_offsets {
        let tag = String::from_utf8_lossy(&rebuilt[offset..(offset + 400).min(rebuilt.len())])
            .split('>')
            .next()
            .unwrap()
            .to_owned();
        assert!(
            tag.ends_with(&format!(" aid=\"{aid}\"")) || tag.ends_with(&format!(" cid=\"{aid}\"")),
            "{tag}"
        );
    }
    (m, files)
}

fn id(m: &Kf8Markup, href: &str, id: &str) -> Option<String> {
    m.id_map.get(&(href.to_owned(), id.to_owned())).cloned()
}

#[test]
fn a_small_file_is_one_skeleton_and_one_fragment_with_aids_on_every_text_element() {
    let body = "<section class=\"scene\" id=\"sc-001\">\n<h2 class=\"scene-heading\">INT. ROOM - DAY</h2>\n<p class=\"action\">A &amp; B.</p>\n</section>\n";
    let (m, files) = check_markup(book_of(&[body]));
    assert_eq!(m.skel_table.len(), 1);
    assert_eq!(
        (
            m.skel_table[0].file_number,
            m.skel_table[0].chunk_count,
            m.skel_table[0].start_pos
        ),
        (0, 1, 0)
    );
    assert_eq!(m.chunk_table[0].selector, "P-//*[@aid='0']");
    assert!(files[0].contains("<body aid=\"0\">"));
    assert!(files[0].contains("<section class=\"scene\" id=\"sc-001\" aid=\"1\">"));
    assert_eq!(id(&m, "text/f0.xhtml", "sc-001").as_deref(), Some("1"));
    assert_eq!(id(&m, "text/f0.xhtml", "").as_deref(), Some("0"));
}

#[test]
fn a_section_larger_than_a_fragment_is_stepped_into_and_big_text_is_cut() {
    let paras: String = (0..120)
        .map(|i| format!("<p class=\"action\">Line {i} of action that runs on for a while.</p>\n"))
        .collect();
    let huge = format!(
        "<p class=\"dialogue\">{}</p>\n",
        "Words, words, words. ".repeat(600)
    );
    let body = format!(
        "<section class=\"scene\" id=\"sc-001\">\n{paras}{huge}</section>\n<section class=\"scene\" id=\"sc-002\">\n<p>Short.</p>\n</section>\n"
    );
    let (m, files) = check_markup(book_of(&[&body]));
    assert!(m.chunk_table.len() > 2);
    assert!(m.chunk_table.iter().any(|c| c.selector.starts_with("S-")));
    assert!(files[0].contains("<span class=\"AmznBigTextBlock\">"));
}

#[test]
fn big_text_is_never_cut_inside_an_entity() {
    for at in [8188, 8189, 8190, 8191] {
        let body = format!(
            "<p class=\"action\">{}&amp;{}</p>\n",
            "a".repeat(at),
            "b".repeat(200)
        );
        let (_, files) = check_markup(book_of(&[&body]));
        assert!(
            files[0].contains("<span class=\"AmznBigTextBlock\">&amp;b"),
            "{at}"
        );
    }
}

#[test]
fn several_files_aids_numbered_per_file_positions_run_on() {
    let (m, _) = check_markup(book_of(&[
        "<p id=\"a\">One</p>\n",
        "<p id=\"b\">Two</p>\n<p>Three</p>\n",
    ]));
    assert_eq!(id(&m, "text/f1.xhtml", "b").as_deref(), Some("UGI1"));
    assert!(m.skel_table[1].start_pos > m.skel_table[0].start_pos);
    assert_eq!(m.chunk_table[1].file_number, 1);
}

#[test]
fn tables_get_no_aid_and_an_id_inside_one_gets_a_cid() {
    let body = "<table class=\"dual-dialogue\"><tr><td id=\"x\"><p>L</p></td><td><p>R</p></td></tr></table>\n";
    let (m, files) = check_markup(book_of(&[body]));
    assert!(files[0].contains("<td id=\"x\" cid=\"c1\">"));
    let at = files[0].find("<table").unwrap();
    assert!(!files[0][at..at + files[0][at..].find('>').unwrap()].contains(" aid="));
    assert_eq!(id(&m, "text/f0.xhtml", "x").as_deref(), Some("c1"));
}

#[test]
fn pictures_become_kindle_embed_and_a_link_to_another_file_a_kindle_pos_fid_link() {
    let mut book = book_of(&[
        "<p><img src=\"../images/a.png\" alt=\"\"/></p>\n<p><a href=\"f1.xhtml#t\">go</a></p>\n",
        "<p id=\"t\">there</p>\n",
    ]);
    let mut resources = HashMap::new();
    resources.insert(
        "images/a.png".to_owned(),
        ResourceRef {
            index: 1,
            mime: "image/png".into(),
        },
    );
    let m = kf8_markup(&mut book, &resources).unwrap();
    let files = rebuild_files(&m);
    assert!(files[0].contains("src=\"kindle:embed:0001?mime=image/png\""));
    assert_eq!(m.used_images, vec!["images/a.png"]);
    let at = files[0].find("href=\"kindle:pos:fid:").expect("a link");
    let link = &files[0][at + 21..at + 21 + 19];
    let (fid, off) = (&link[..4], &link[9..]);
    assert_eq!(&link[4..9], ":off:");
    let target = m.aid_offsets[&id(&m, "text/f1.xhtml", "t").unwrap()];
    assert_eq!(usize::from_str_radix(fid, 32).unwrap(), target.0);
    assert_eq!(usize::from_str_radix(off, 32).unwrap(), target.1);
}

#[test]
fn epub_type_loses_its_prefix_and_the_namespace_declarations_go() {
    let body = "<section epub:type=\"titlepage\" class=\"titlepage\"><h1>T</h1></section>\n";
    let (_, files) = check_markup(book_of(&[body]));
    assert!(files[0].contains("<section type=\"titlepage\" class=\"titlepage\" aid=\"1\">"));
    assert!(!files[0].contains("xmlns:epub"));
}

#[test]
fn every_committed_fixture_rebuilds_whole() {
    for name in ["field-station", "images", "torture", "screenplay", "jpx"] {
        check_markup(read_epub(&fixture_epub(name)).unwrap());
    }
}
