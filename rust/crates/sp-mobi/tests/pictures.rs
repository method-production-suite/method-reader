//! The picture gate and the picture records, against the oracle's own
//! pictures (ports tests/mobi.test.ts's "images.pdf in the MOBI", and keeps
//! the records proven while MOBI_CARRIES_PICTURES is shut).

mod common;

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use common::*;
use sp_mobi::{
    MOBI_CARRIES_PICTURES, MobiInput, MobiMeta, build_mobi, mobi_art, tokens_to_mobi_html,
};
use sp_model::{DEFAULT_FORMAT_OPTIONS, Token};

/// A dump's tokens, title and author, and its picture folder.
fn dump(name: &str) -> (Vec<Token>, MobiMeta, BTreeMap<String, Vec<u8>>) {
    let dir = workspace().join("oracle").join(name);
    let tokens = serde_json::from_slice(&std::fs::read(dir.join("tokens.json")).unwrap()).unwrap();
    let convert: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("convert.json")).unwrap()).unwrap();
    let meta = MobiMeta {
        title: convert["title"].as_str().unwrap().to_owned(),
        author: convert["author"].as_str().map(str::to_owned),
    };
    let mut pictures = BTreeMap::new();
    for e in std::fs::read_dir(dir.join("images")).unwrap() {
        let p = e.unwrap().path();
        pictures.insert(file_name(&p), std::fs::read(&p).unwrap());
    }
    (tokens, meta, pictures)
}

fn file_name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

/// The book renderBooks builds with the gate OPEN (`mobiPictures: true`).
fn open_book(name: &str) -> (Vec<u8>, sp_mobi::MobiArt, String) {
    let (tokens, meta, pictures) = dump(name);
    let have: HashSet<String> = pictures.keys().cloned().collect();
    let art = mobi_art(&tokens, &DEFAULT_FORMAT_OPTIONS, &have);
    let html = tokens_to_mobi_html(&tokens, &meta, &DEFAULT_FORMAT_OPTIONS, &art);
    let images = art.order.iter().map(|f| pictures[f].clone()).collect();
    let mobi = build_mobi(&MobiInput {
        title: meta.title,
        author: meta.author,
        html: html.clone(),
        images,
        cover_index: art.cover_index,
        time: Some(1_767_225_600),
    })
    .unwrap();
    (mobi, art, html)
}

#[test]
fn the_gate_is_shut() {
    // The one line to change when the device probe passes (registry #19a).
    const { assert!(!MOBI_CARRIES_PICTURES) };
}

#[test]
fn with_the_gate_open_each_picture_book_is_the_typescripts_byte_for_byte() {
    // SHA-256 of the TypeScript's book for the same tokens, pictures and
    // clock, with mobiPictures on (computed with bun 1.3.14, 2026-10-11).
    for (name, want) in [
        (
            "images",
            "a3722c8b93eff26d02cc1f23cd7a76266e51bcc9f67d15a6a08a90a80db23de6",
        ),
        (
            "jpx",
            "406c781e5c7fc8dab695746340212b6c102aca8d299ae8265728f2e44ca81837",
        ),
        (
            "torture",
            "a719de4da097a2c86a4a8752ef6f8574da8e7206480c9736d12c2c59a2df6800",
        ),
    ] {
        assert_eq!(sha256_hex(&open_book(name).0), want, "{name}");
    }
}

#[test]
fn every_recindex_opens_the_record_holding_that_very_picture() {
    let (mobi, _, html) = open_book("images");
    let refs: Vec<usize> = html
        .match_indices("<img recindex=\"")
        .map(|(i, m)| html[i + m.len()..i + m.len() + 5].parse().unwrap())
        .collect();
    assert_eq!(refs, vec![1, 2]);
    let (_, _, pictures) = dump("images");
    assert_eq!(
        pictures.keys().cloned().collect::<Vec<_>>(),
        vec!["p001-01.png", "p002-01.png"]
    );
    // The script's own order, read off the .fountain rather than asked of
    // mobi_art, so this does not grade mobi_art against itself.
    let fountain =
        std::fs::read_to_string(workspace().join("oracle/images/script.fountain")).unwrap();
    let want: Vec<&Vec<u8>> = fountain
        .lines()
        .filter_map(|l| l.strip_prefix("[[img: "))
        .map(|rest| &pictures[rest.split_whitespace().next().unwrap()])
        .collect();
    assert_eq!(want.len(), 2);
    let recs = records(&mobi);
    let first = header(&mobi).first_image as usize;
    for (i, n) in refs.iter().enumerate() {
        assert_eq!(&recs[first + n - 1], want[i]);
    }
    // The title card is named as the cover, in the first image record.
    assert_eq!(exth(&mobi, 201), Some(vec![0, 0, 0, 0]));
    assert_eq!(
        mobi_pictures(&mobi),
        want.into_iter().cloned().collect::<Vec<_>>()
    );
}

#[test]
fn behind_the_gate_the_clis_book_carries_no_picture_and_no_cover() {
    for name in ["images", "jpx", "torture"] {
        let mobi = std::fs::read(workspace().join("oracle").join(name).join("book.mobi")).unwrap();
        assert!(mobi_pictures(&mobi).is_empty(), "{name}");
        assert!(exth(&mobi, 201).is_none(), "{name}");
        assert!(!mobi_html(&mobi).contains("recindex"), "{name}");
    }
}
