//! The writer's parts, each on its own: ports tests/azw3-index.test.ts,
//! azw3-palmdoc.test.ts, azw3-tbs.test.ts, azw3-xml.test.ts and
//! azw3-epub.test.ts.

mod common;

use std::collections::HashSet;
use std::io::Read;

use common::fixture_epub;
use common::kf8_reader::{decint, read_index};
use common::palmdoc_decode::palmdoc_decompress;
use sp_azw3::epub::{read_epub, resolve_href};
use sp_azw3::index::{
    ChunkEntry, Cncx, GuideEntry, NcxEntry, SkelEntry, chunk_index, encint, guide_index, ncx_index,
    skel_index, to_base,
};
use sp_azw3::palmdoc::palmdoc_compress;
use sp_azw3::tbs::{TbsEntry, encode_trailing_data, trailing_byte_sequences};
use sp_azw3::xml::{XNode, parse_xml, serialize, serialize_element};

// ── azw3-index ──

#[test]
fn encint_round_trips_both_ways_and_matches_utils_py() {
    assert_eq!(encint(0x11111, true), vec![0x04, 0x22, 0x91]);
    assert_eq!(encint(0x11111, false), vec![0x84, 0x22, 0x11]);
    for n in [
        0u64,
        1,
        127,
        128,
        16383,
        16384,
        (1 << 31) - 1,
        (1 << 32) + 5,
    ] {
        assert_eq!(decint(&encint(n, true), true), (n, encint(n, true).len()));
        assert_eq!(
            decint(&encint(n, false), false),
            (n, encint(n, false).len())
        );
    }
}

#[test]
fn to_base_is_base_32_upper_case_zero_padded() {
    assert_eq!(to_base(0, 32, 4), "0000");
    assert_eq!(to_base(1, 32, 4), "0001");
    assert_eq!(to_base(32, 32, 4), "0010");
    assert_eq!(to_base(1_000_000, 32, 0), "UGI0");
    assert_eq!(to_base(31, 32, 10), "000000000V");
}

#[test]
fn cncx_offsets_point_at_each_string_deduplicated_and_roll_over() {
    // `${i} `.padEnd(400, 'x')
    let many: Vec<String> = (0..300)
        .map(|i| format!("{:x<400}", format!("{i} ")))
        .collect();
    let mut strings: Vec<&str> = vec!["a", "b", "a"];
    strings.extend(many.iter().map(String::as_str));
    let cncx = Cncx::new(strings);
    assert_eq!(cncx.records.len(), 2);
    assert_eq!(cncx.offset_of("a").unwrap(), 0);
    assert_eq!(cncx.offset_of("b").unwrap(), 2);
    assert!(cncx.offset_of(&many[299]).unwrap() >= 0x10000);
}

#[test]
fn skeleton_index_reads_back() {
    let records = skel_index(&[
        SkelEntry {
            file_number: 0,
            chunk_count: 1,
            start_pos: 0,
            length: 300,
        },
        SkelEntry {
            file_number: 1,
            chunk_count: 3,
            start_pos: 900,
            length: 280,
        },
    ])
    .unwrap();
    assert_eq!(records.len(), 2);
    let t = read_index(&records, 0).unwrap();
    let names: Vec<&str> = t.entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["SKEL0000000000", "SKEL0000000001"]);
    let e = t.get("SKEL0000000001").unwrap();
    assert_eq!(e[&1], vec![3, 3]);
    assert_eq!(e[&6], vec![900, 280, 900, 280]);
}

fn chunk(
    insert_pos: usize,
    selector: &str,
    sequence_number: usize,
    start_pos: usize,
    length: usize,
) -> ChunkEntry {
    ChunkEntry {
        insert_pos,
        selector: selector.to_owned(),
        file_number: 0,
        sequence_number,
        start_pos,
        length,
    }
}

#[test]
fn fragment_index_reads_back() {
    let records = chunk_index(&[
        chunk(214, "P-//*[@aid='0']", 0, 0, 4000),
        chunk(4214, "S-//*[@aid='1F']", 1, 4000, 60),
    ])
    .unwrap();
    let t = read_index(&records, 0).unwrap();
    let names: Vec<&str> = t.entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["0000000214", "0000004214"]);
    let second = t.get("0000004214").unwrap();
    assert_eq!(t.cncx[&second[&2][0]], "S-//*[@aid='1F']");
    assert_eq!(second[&3], vec![0]);
    assert_eq!(second[&4], vec![1]);
    assert_eq!(second[&6], vec![4000, 60]);
}

#[test]
fn guide_and_ncx_read_back() {
    let guide = read_index(
        &guide_index(&[GuideEntry {
            kind: "text".into(),
            title: "Begin Reading".into(),
            pos_fid: (3, 0),
        }])
        .unwrap(),
        0,
    )
    .unwrap();
    assert_eq!(
        guide.cncx[&guide.get("text").unwrap()[&1][0]],
        "Begin Reading"
    );
    assert_eq!(guide.get("text").unwrap()[&6], vec![3, 0]);

    let entry = |index, offset, length, label: &str, pos_fid| NcxEntry {
        index,
        offset,
        length,
        label: label.to_owned(),
        depth: 0,
        parent: None,
        first_child: None,
        last_child: None,
        pos_fid,
    };
    let ncx = read_index(
        &ncx_index(&[
            entry(0, 300, 500, "INT. ROOM - DAY", (1, 0)),
            entry(1, 800, 90, "EXT. ROAD - NIGHT", (2, 14)),
        ])
        .unwrap(),
        0,
    )
    .unwrap();
    let names: Vec<&str> = ncx.entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["00", "01"]);
    let e = ncx.get("01").unwrap();
    assert_eq!(e[&1], vec![800]);
    assert_eq!(e[&2], vec![90]);
    assert_eq!(ncx.cncx[&e[&3][0]], "EXT. ROAD - NIGHT");
    assert_eq!(e[&4], vec![0]);
    assert_eq!(e[&6], vec![2, 14]);
}

#[test]
fn a_large_index_rolls_over_into_more_records_and_still_reads_as_one() {
    let selectors: Vec<String> = (0..6000).map(|i| format!("P-//*[@aid='{i}']")).collect();
    let table: Vec<ChunkEntry> = (0..6000)
        .map(|i| chunk(i * 9000, &selectors[i], i, i * 8000, 8000))
        .collect();
    let records = chunk_index(&table).unwrap();
    let t = read_index(&records, 0).unwrap();
    assert!(t.record_count > 1);
    assert_eq!(t.entries.len(), 6000);
    for r in &records {
        assert!(r.len() <= 0x10000);
    }
}

// ── azw3-palmdoc ──

fn round_trips(bytes: &[u8]) -> Vec<u8> {
    let packed = palmdoc_compress(bytes);
    assert_eq!(palmdoc_decompress(&packed), bytes);
    packed
}

#[test]
fn repetitive_markup_shrinks_and_comes_back() {
    let html = "<div class=\"dialogue-block\"><p class=\"character\">JO</p><p class=\"dialogue\">Again.</p></div>\n".repeat(40);
    let bytes = &html.as_bytes()[..html.len().min(4096)];
    assert!(round_trips(bytes).len() < bytes.len() / 2);
}

#[test]
fn the_space_plus_letter_pair_is_one_byte() {
    assert!(round_trips(b"a b c d e f g").len() < "a b c d e f g".len());
}

#[test]
fn bytes_outside_the_ascii_range_travel_as_literal_runs() {
    round_trips(
        "Mara\u{2019}s caf\u{e9} \u{2014} \u{201c}quoted\u{201d} \u{fc}n\u{ef}c\u{f6}d\u{e9} "
            .repeat(20)
            .as_bytes(),
    );
    round_trips(&[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 0x80, 0xff, 0x20, 0x41, 0x20, 0x7f, 0x20,
    ]);
}

#[test]
fn every_byte_value_and_the_edges_of_a_record() {
    let all: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(7919) & 0xff) as u8)
        .collect();
    round_trips(&all);
    round_trips(&[]);
    round_trips(b"x");
    round_trips(b"abcabcabcabc");
}

#[test]
fn a_match_never_reaches_further_back_than_2047_bytes() {
    let block = b"<p class=\"unique-run\">0123456789</p>";
    let filler: Vec<u8> = (0..2100usize)
        .map(|i| 0x41 + ((i * 31) % 26) as u8)
        .collect();
    round_trips(&[&block[..], &filler, &block[..]].concat());
}

// ── azw3-tbs ──

fn tbs(index: usize, start: usize, length: usize) -> TbsEntry {
    TbsEntry {
        index,
        start,
        length,
        depth: 0,
        parent: None,
    }
}

#[test]
fn a_flat_toc_over_three_records_worked_by_hand() {
    let out = trailing_byte_sequences(
        &[tbs(0, 300, 500), tbs(1, 800, 6000), tbs(2, 6800, 2200)],
        &[4096, 4096, 1000],
    );
    assert_eq!(
        out,
        vec![
            vec![(6) | 0x80, 0x88, 2],
            vec![(1 << 3 | 6) | 0x80, 0x88, 2],
            vec![(2 << 3 | 2) | 0x80, 0x88]
        ]
    );
}

#[test]
fn a_record_no_entry_touches_gets_an_empty_sequence() {
    assert!(trailing_byte_sequences(&[tbs(0, 10, 5)], &[100, 100])[1].is_empty());
}

#[test]
fn an_entry_spanning_a_whole_record_is_flagged_as_spanning() {
    let out = trailing_byte_sequences(&[tbs(0, 0, 9000)], &[4096, 4096, 4096]);
    assert_eq!(out[1], vec![3 | 0x80, 0x88, 0x80]);
}

#[test]
fn a_trailing_entry_ends_with_its_own_length_read_backward() {
    assert_eq!(encode_trailing_data(&[]), vec![0x81]);
    assert_eq!(encode_trailing_data(&[1, 2, 3]), vec![1, 2, 3, 0x84]);
    let long = encode_trailing_data(&[0; 200]);
    assert_eq!(long.len(), 202);
    assert_eq!(&long[200..], &[0x81, 0x4a]);
}

// ── azw3-xml ──

#[test]
fn elements_attributes_text_and_entities() {
    let doc = parse_xml(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"x\"><body><p class=\"a\" id='b'>Tom &amp; Jerry&apos;s &#x2014;&#8212; &lt;end&gt;</p><br/>tail</body></html>",
    )
    .unwrap();
    assert_eq!(doc.root.name, "html");
    let p = doc.root.find_all("p")[0];
    assert_eq!(p.get_attr("class"), Some("a"));
    assert_eq!(p.get_attr("id"), Some("b"));
    assert_eq!(p.text_content(), "Tom & Jerry's \u{2014}\u{2014} <end>");
    assert!(doc.root.text_content().ends_with("tail"));
}

#[test]
fn comments_and_processing_instructions_are_dropped_and_the_text_joins() {
    let doc = parse_xml("<a>one<!-- gone -->two<?pi x?>three</a>").unwrap();
    assert_eq!(doc.root.children, vec![XNode::Text("onetwothree".into())]);
}

#[test]
fn anything_malformed_fails_rather_than_guessing() {
    assert!(parse_xml("<a><b></a>").is_err());
    assert!(parse_xml("<a>").is_err());
    assert!(parse_xml("<a>&nbsp;</a>").unwrap_err().0.contains("entity"));
    assert!(parse_xml("<a x=1/>").is_err());
    assert!(parse_xml("<a/><b/>").unwrap_err().0.contains("second root"));
}

#[test]
fn a_subtree_serializes_to_exactly_the_bytes_it_occupies_in_the_whole() {
    let doc = parse_xml("<r><p a=\"1&amp;2&quot;\">x &lt; y<i/></p>\n<q>z</q></r>").unwrap();
    let whole = serialize_element(&doc.root, None);
    for d in doc.root.descendants() {
        assert!(whole.contains(&serialize_element(d, None)));
    }
    assert_eq!(
        whole,
        "<r><p a=\"1&amp;2&quot;\">x &lt; y<i/></p>\n<q>z</q></r>"
    );
}

#[test]
fn close_empty_writes_text_elements_open_and_shut() {
    let doc = parse_xml("<r><p/><img/></r>").unwrap();
    let set: HashSet<&str> = ["p"].into_iter().collect();
    assert_eq!(
        serialize(&XNode::Element(doc.root), Some(&set)),
        "<r><p></p><img/></r>"
    );
}

#[test]
fn reads_every_document_our_own_epub_writes_and_writes_the_same_text_back() {
    for name in ["field-station", "images", "jpx", "screenplay", "torture"] {
        let bytes = fixture_epub(name);
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut docs = 0;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).unwrap();
            let n = f.name().unwrap().into_owned();
            if !(n.ends_with(".xhtml") || n.ends_with(".opf") || n.ends_with(".xml")) {
                continue;
            }
            docs += 1;
            let mut xml = String::new();
            f.read_to_string(&mut xml).unwrap();
            let once = serialize_element(&parse_xml(&xml).unwrap().root, None);
            assert_eq!(
                serialize_element(&parse_xml(&once).unwrap().root, None),
                once,
                "{name}: {n}"
            );
            assert_eq!(
                parse_xml(&once).unwrap().root.text_content(),
                parse_xml(&xml).unwrap().root.text_content()
            );
        }
        assert!(docs >= 4, "{name}");
    }
}

// ── azw3-epub ──

#[test]
fn hrefs_resolve_against_their_folder() {
    assert_eq!(
        resolve_href("text", "../images/p002-01.png").unwrap(),
        "images/p002-01.png"
    );
    assert_eq!(
        resolve_href("", "text/body001.xhtml#sc-001").unwrap(),
        "text/body001.xhtml#sc-001"
    );
    assert_eq!(
        resolve_href("OEBPS/text", "./a%20b.xhtml").unwrap(),
        "OEBPS/text/a b.xhtml"
    );
    assert!(resolve_href("", "a%zz.xhtml").is_err());
}

#[test]
fn field_station_metadata_spine_stylesheet_and_scene_toc() {
    let book = read_epub(&fixture_epub("field-station")).unwrap();
    assert!(!book.title.is_empty());
    assert_eq!(book.authors.len(), 1);
    assert_eq!(book.language, "en");
    assert!(book.identifier.as_deref().unwrap().starts_with("urn:"));
    assert_eq!(book.spine[0].href, "titlepage.xhtml");
    assert!(
        book.spine[1..]
            .iter()
            .all(|s| s.href.starts_with("text/body"))
    );
    assert_eq!(
        book.stylesheets
            .iter()
            .map(|(h, _)| h.as_str())
            .collect::<Vec<_>>(),
        vec!["style.css"]
    );
    assert_eq!(book.toc.len(), 11);
    for t in &book.toc {
        let (file, id) = t.href.split_once('#').unwrap();
        assert!(
            file.starts_with("text/body")
                && file.ends_with(".xhtml")
                && id.starts_with("sc-")
                && t.depth == 0
        );
        let doc = &book.spine.iter().find(|s| s.href == file).unwrap().doc;
        assert!(
            doc.root
                .find_all("section")
                .iter()
                .any(|s| s.get_attr("id") == Some(id))
        );
    }
    assert_eq!(
        book.landmarks
            .iter()
            .map(|l| l.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["titlepage", "bodymatter"]
    );
    assert!(book.images.is_empty());
    assert!(book.cover_href.is_none());
}

#[test]
fn images_both_pictures_in_manifest_order_and_the_cover_named() {
    let book = read_epub(&fixture_epub("images")).unwrap();
    assert_eq!(
        book.images
            .iter()
            .map(|i| i.href.as_str())
            .collect::<Vec<_>>(),
        vec!["images/p001-01.png", "images/p002-01.png"]
    );
    assert!(
        book.images
            .iter()
            .all(|i| i.media_type == "image/png" && i.bytes[1] == 0x50)
    );
    assert_eq!(book.cover_href.as_deref(), Some("images/p001-01.png"));
}
