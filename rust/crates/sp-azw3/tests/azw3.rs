//! Method Reader's own AZW3, read back by the structural reader
//! (tests/common/kf8_reader.rs): no Calibre needed, so this is what CI
//! proves. Ports tests/azw3.test.ts and tests/azw3-revision.test.ts's
//! writer half (the staleness rule that reads the stamp is sp-engine's).

mod common;

use common::kf8_reader::{Kf8Book, read_kf8};
use common::{fixture_epub, mask_pdb_dates};
use sp_azw3::epub::read_epub;
use sp_azw3::xml::parse_xml;
use sp_azw3::{
    AZW3_REVISION_EXTH, AZW3_WRITER_REVISION, azw3_revision, azw3_revision_of_file, build_azw3_at,
};
use sp_mobi::palmdb::{ByteWriter, exth_number, exth_record, palm_database};

const T: u64 = 1_767_225_600;

fn built(name: &str) -> (Vec<u8>, Vec<u8>, Kf8Book) {
    let epub = fixture_epub(name);
    let azw3 = build_azw3_at(&epub, T).unwrap();
    let book = read_kf8(&azw3).unwrap();
    (epub, azw3, book)
}

fn norm(s: &str) -> String {
    s.split(sp_fountain::js::is_space)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn epub_texts(epub: &[u8]) -> Vec<String> {
    read_epub(epub)
        .unwrap()
        .spine
        .iter()
        .map(|s| norm(&s.doc.root.find_all("body")[0].text_content()))
        .collect()
}

fn azw3_texts(book: &Kf8Book) -> Vec<String> {
    book.parts
        .iter()
        .map(|p| norm(&parse_xml(p).unwrap().root.find_all("body")[0].text_content()))
        .collect()
}

#[test]
fn each_fixture_is_a_well_formed_kf8_file_carrying_every_word_in_order() {
    for name in ["field-station", "images", "torture", "screenplay", "jpx"] {
        let (epub, _, book) = built(name);
        assert_eq!(book.header.version, 8);
        assert_eq!(book.header.compression, 2);
        assert_eq!(book.header.book_type, 2);
        assert_eq!(book.header.extra_data_flags, 0b11);
        assert_eq!(azw3_texts(&book), epub_texts(&epub), "{name}");
        for p in &book.parts {
            assert!(p.contains("href=\"kindle:flow:0001?mime=text/css\""));
        }
        assert_eq!(book.flows.len(), 2);
        assert!(String::from_utf8_lossy(&book.flows[1]).contains(".dialogue-block"));
        assert!(
            book.trailing
                .iter()
                .all(|t| t.overlap.len() <= 3 && t.tbs.is_some())
        );
    }
}

#[test]
fn the_stylesheet_travels_verbatim_keeps_and_all() {
    let (epub, _, book) = built("field-station");
    let e = read_epub(&epub).unwrap();
    let css = &e.stylesheets[0].1;
    assert_eq!(String::from_utf8(book.flows[1].clone()).unwrap(), *css);
    assert!(css.contains("break-after: avoid"));
}

#[test]
fn metadata_title_author_language_asin_and_ebok() {
    let (epub, _, book) = built("field-station");
    let e = read_epub(&epub).unwrap();
    assert_eq!(book.header.title, e.title);
    assert_eq!(book.exth_string(503), vec![e.title.clone()]);
    assert_eq!(book.exth_string(100), e.authors);
    assert_eq!(book.exth_string(524), vec!["en"]);
    assert_eq!(book.header.language, 9);
    assert_eq!(book.exth_string(501), vec!["EBOK"]);
    let id = e.identifier.unwrap();
    assert_eq!(
        book.exth_string(113),
        vec![id.rsplit(':').next().unwrap().to_owned()]
    );
    assert_eq!(book.exth_string(528), vec!["true"]);
}

#[test]
fn the_toc_one_ncx_entry_per_scene_each_pointing_at_its_scene() {
    let (epub, _, book) = built("field-station");
    let e = read_epub(&epub).unwrap();
    assert_eq!(
        book.ncx.iter().map(|n| n.label.clone()).collect::<Vec<_>>(),
        e.toc.iter().map(|t| t.title.clone()).collect::<Vec<_>>()
    );
    assert!(book.ncx.iter().all(|n| n.depth == 0));
    let rebuilt = book.parts.concat().into_bytes();
    for (i, t) in e.toc.iter().enumerate() {
        let n = &book.ncx[i];
        let off = n.offset as usize;
        let tag = String::from_utf8_lossy(&rebuilt[off..(off + 200).min(rebuilt.len())])
            .split('>')
            .next()
            .unwrap()
            .to_owned();
        assert!(tag.contains(&format!("id=\"{}\"", t.href.split('#').nth(1).unwrap())));
        let chunk = &book.chunks[n.pos as usize];
        assert_eq!(chunk.insert_pos + n.fid, n.offset);
        if i > 0 {
            assert!(n.offset > book.ncx[i - 1].offset);
        }
        assert!(n.length > 0);
    }
}

#[test]
fn the_guide_starts_reading_at_the_body_past_the_title_page() {
    let (_, _, book) = built("field-station");
    assert_eq!(
        book.guide
            .iter()
            .map(|g| g.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["text"]
    );
    let start = book.exth_number(116);
    assert_eq!(start.len(), 1);
    let rebuilt = book.parts.concat().into_bytes();
    let s = start[0] as usize;
    assert_eq!(&rebuilt[s..s + 5], b"<body");
    assert!(u64::from(start[0]) > book.skel[0].start + book.skel[0].length);
}

#[test]
fn the_title_page_is_the_first_file_and_scene_sections_survive() {
    let (_, _, book) = built("field-station");
    assert!(book.parts[0].contains("class=\"titlepage\""));
    let rest = book.parts[1..].concat();
    let at = rest
        .find("<section class=\"scene\" id=\"sc-001\" aid=\"")
        .expect("the first scene");
    let aid: String = rest[at + 41..].chars().take_while(|c| *c != '"').collect();
    assert!(
        !aid.is_empty()
            && aid
                .chars()
                .all(|c| c.is_ascii_digit() || ('A'..='V').contains(&c))
    );
}

#[test]
fn pictures_every_one_a_resource_shown_by_kindle_embed_with_the_cover_in_exth_201() {
    let (epub, _, book) = built("images");
    let e = read_epub(&epub).unwrap();
    assert_eq!(book.resources.len(), e.images.len());
    for (i, img) in e.images.iter().enumerate() {
        assert_eq!(book.resources[i], img.bytes);
    }
    assert_eq!(book.exth_number(125), vec![e.images.len() as u32]);
    let cover = e
        .images
        .iter()
        .position(|i| Some(&i.href) == e.cover_href.as_ref())
        .unwrap();
    assert_eq!(book.exth_number(201), vec![cover as u32]);
    assert_eq!(book.exth_number(203), vec![0]);
    let all = book.parts.concat();
    let shown = regex::Regex::new(r"kindle:embed:[0-9A-V]{4}\?mime=image/png")
        .unwrap()
        .find_iter(&all)
        .count();
    let imgs: usize = e
        .spine
        .iter()
        .map(|s| s.doc.root.find_all("img").len())
        .sum();
    assert_eq!(shown, imgs);
}

#[test]
fn a_book_with_no_pictures_has_no_resources_and_no_cover() {
    let (_, _, book) = built("field-station");
    assert_eq!(book.header.first_resource, 0xffff_ffff);
    assert!(book.exth_value(201).is_empty());
    assert!(book.exth_value(131).is_empty());
}

#[test]
fn two_builds_of_one_epub_are_the_same_file_but_for_the_palmdb_dates() {
    let epub = fixture_epub("screenplay");
    let a = build_azw3_at(&epub, T).unwrap();
    let b = build_azw3_at(&epub, T + 86_400).unwrap();
    assert_ne!(a, b);
    assert_eq!(mask_pdb_dates(&a), mask_pdb_dates(&b));
}

#[test]
fn the_build_time_is_source_date_epoch_in_the_palmdb_header() {
    let (_, azw3, _) = built("screenplay");
    assert_eq!(
        u32::from_be_bytes(azw3[36..40].try_into().unwrap()),
        T as u32
    );
    assert_eq!(
        u32::from_be_bytes(azw3[40..44].try_into().unwrap()),
        T as u32
    );
}

// ── the writer revision (tests/azw3-revision.test.ts) ──

/// The smallest file the AZW3 staleness rule reads as one of ours
/// (tests/helpers/stamped-azw3.ts): a PalmDB whose record 0 is a MOBI header
/// with an EXTH block carrying the writer revision, or none.
fn stamped_azw3(revision: Option<u32>) -> Vec<u8> {
    let mut records = vec![exth_record(503, b"T")];
    if let Some(r) = revision {
        records.push(exth_number(AZW3_REVISION_EXTH, r));
    }
    let size: usize = records.iter().map(Vec::len).sum();
    let mut exth = ByteWriter::new();
    exth.ascii("EXTH")
        .u32((size + 12) as u32)
        .u32(records.len() as u32);
    for r in &records {
        exth.bytes(r);
    }
    exth.zeros(4 - (size % 4));
    let mut r0 = ByteWriter::new();
    r0.u16(2)
        .zeros(2)
        .u32(0)
        .u16(0)
        .u16(4096)
        .zeros(4)
        .ascii("MOBI")
        .u32(264)
        .u32(2)
        .u32(65001)
        .u32(1)
        .u32(8)
        .zeros(128 - 40)
        .u32(0x50)
        .zeros(264 - (132 - 16))
        .bytes(exth.as_slice());
    palm_database("T", 1, &[r0.into_vec()], 0)
}

#[test]
fn the_revision_is_a_positive_whole_number_in_an_exth_record_no_reader_reads() {
    const { assert!(AZW3_WRITER_REVISION >= 1) };
    const { assert!(AZW3_REVISION_EXTH > 1000) };
}

#[test]
fn every_azw3_we_write_carries_it_and_it_reads_back() {
    let (_, azw3, book) = built("screenplay");
    assert_eq!(
        book.exth_number(AZW3_REVISION_EXTH),
        vec![AZW3_WRITER_REVISION]
    );
    assert_eq!(azw3_revision(&azw3), Some(AZW3_WRITER_REVISION));
}

#[test]
fn a_file_with_no_revision_or_no_mobi_at_all_reads_as_none() {
    assert_eq!(azw3_revision(&stamped_azw3(None)), None);
    assert_eq!(azw3_revision(&stamped_azw3(Some(7))), Some(7));
    assert_eq!(azw3_revision(b"not a book"), None);
    assert_eq!(azw3_revision(&[]), None);
    assert_eq!(
        azw3_revision(&stamped_azw3(Some(AZW3_WRITER_REVISION))[..300]),
        None
    );
    // The engine's MOBI 6 has an EXTH block too, but no revision.
    let mobi = sp_mobi::build_mobi(&sp_mobi::MobiInput {
        title: "T".into(),
        html: "<p>x</p>".into(),
        time: Some(0),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(azw3_revision(&mobi), None);
    let scratch = common::calibre::Scratch::new("azw3-revision");
    assert_eq!(
        azw3_revision_of_file(&scratch.path.join("missing.azw3")),
        None
    );
    let path = scratch.path.join("stamped.azw3");
    std::fs::write(&path, stamped_azw3(Some(3))).unwrap();
    assert_eq!(azw3_revision_of_file(&path), Some(3));
    let (_, azw3, _) = built("images");
    std::fs::write(&path, &azw3).unwrap();
    assert_eq!(azw3_revision_of_file(&path), Some(AZW3_WRITER_REVISION));
}
