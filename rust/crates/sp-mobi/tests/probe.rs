//! The MOBI picture probe's book builder (ports the MOBI half of
//! tests/mobi-picture-probe.test.ts; the EPUB half waits for sp-epub, and
//! the run-as-a-process half for the xtask command).

mod common;

use std::collections::HashSet;

use common::{mobi_html, mobi_pictures};
use sp_mobi::probe::{
    BUILT_AT, EPUB_FILE, MOBI_FILE, build_probe, plain_size, probe_pictures, probe_script,
    single_mobi_file,
};

const SINGLE_FILES: [&str; 3] = [
    "method-reader-picture-probe-2kb.mobi",
    "method-reader-picture-probe-200kb.mobi",
    "method-reader-picture-probe-1400kb.mobi",
];

fn pdb_name(b: &[u8]) -> String {
    String::from_utf8_lossy(&b[..32])
        .trim_end_matches('\0')
        .to_owned()
}

#[test]
fn the_files_and_titles_are_the_ones_the_device_check_names() {
    assert_eq!(MOBI_FILE, "method-reader-picture-probe.mobi");
    assert_eq!(EPUB_FILE, "method-reader-picture-probe.epub");
    let files: Vec<String> = probe_pictures().iter().map(single_mobi_file).collect();
    assert_eq!(files, SINGLE_FILES);
}

#[test]
fn each_single_book_holds_its_one_picture_the_same_bytes_as_the_combined_books() {
    let books = build_probe();
    let combined = mobi_pictures(&books.mobi);
    assert_eq!(books.singles.len(), 3);
    for (i, single) in books.singles.iter().enumerate() {
        assert_eq!(single.file, SINGLE_FILES[i]);
        assert_eq!(
            mobi_pictures(&single.mobi),
            vec![combined[i].clone()],
            "{}",
            SINGLE_FILES[i]
        );
        let html = mobi_html(&single.mobi);
        let line = html
            .find(&format!(
                "<p>The next picture is about {}.",
                plain_size(combined[i].len())
            ))
            .expect("the line naming its size");
        let img = html.find("<img recindex=\"00001\"").expect("the picture");
        assert!(img > line, "{}", SINGLE_FILES[i]);
    }
    assert!(mobi_html(&books.singles[2].mobi).contains(
        "<mbp:pagebreak/>\n<p align=\"center\"><img recindex=\"00001\" alt=\"Illustration\"/></p>\n<mbp:pagebreak/>"
    ));
}

#[test]
fn the_four_books_carry_four_titles_and_four_ids() {
    let books = build_probe();
    let all: Vec<&Vec<u8>> = std::iter::once(&books.mobi)
        .chain(books.singles.iter().map(|s| &s.mobi))
        .collect();
    let names: Vec<String> = all.iter().map(|b| pdb_name(b)).collect();
    assert_eq!(
        names,
        vec![
            "Picture Probe",
            "Picture Probe 2 KB",
            "Picture Probe 200 KB",
            "Picture Probe 1.4 MB"
        ]
    );
    let ids: HashSet<u32> = all
        .iter()
        .map(|b| u32::from_be_bytes([b[68], b[69], b[70], b[71]]))
        .collect();
    assert_eq!(ids.len(), 4);
    for b in all {
        assert_eq!(u32::from_be_bytes([b[36], b[37], b[38], b[39]]), BUILT_AT);
    }
}

#[test]
fn the_pictures_land_where_the_device_check_needs_them() {
    let sizes: Vec<usize> = mobi_pictures(&build_probe().mobi)
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(sizes.len(), 3);
    assert!((1_500..2_500).contains(&sizes[0]), "{sizes:?}");
    // The middle one's floor is the point of it: past the 127 KB cap with
    // room to spare, in binary kilobytes as well as decimal.
    const CAP: usize = 127 * 1024;
    assert!(sizes[1] > CAP, "{sizes:?}");
    assert!((150_000..250_000).contains(&sizes[1]), "{sizes:?}");
    assert!((1_200_000..1_600_000).contains(&sizes[2]), "{sizes:?}");
}

#[test]
fn the_records_hold_the_three_pngs_in_the_order_the_book_shows_them() {
    let books = build_probe();
    let records = mobi_pictures(&books.mobi);
    for p in &records {
        assert_eq!(&p[..8], &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(&p[12..16], b"IHDR");
    }
    let drawn: Vec<Vec<u8>> = probe_pictures().into_iter().map(|p| p.bytes).collect();
    assert_eq!(records, drawn);
}

#[test]
fn each_picture_follows_the_line_that_names_its_size_and_the_largest_is_a_plate() {
    let books = build_probe();
    let html = mobi_html(&books.mobi);
    let records = mobi_pictures(&books.mobi);
    let mut from = 0;
    for (i, picture) in records.iter().enumerate() {
        let line = from
            + html[from..]
                .find(&format!(
                    "<p>The next picture is about {}.",
                    plain_size(picture.len())
                ))
                .expect("the line");
        let img = from
            + html[from..]
                .find(&format!("<img recindex=\"0000{}\"", i + 1))
                .expect("the picture");
        assert!(img > line, "picture {}", i + 1);
        from = img;
    }
    assert!(html.contains(
        "<mbp:pagebreak/>\n<p align=\"center\"><img recindex=\"00003\" alt=\"Illustration\"/></p>\n<mbp:pagebreak/>"
    ));
    for n in ["00001", "00002"] {
        assert!(!html.contains(&format!(
            "<img recindex=\"{n}\" alt=\"Illustration\"/></p>\n<mbp:pagebreak/>"
        )));
    }
}

#[test]
fn every_run_writes_the_same_bytes() {
    assert_eq!(build_probe(), build_probe());
}

#[test]
fn the_script_is_the_typescripts() {
    // tools/mobi-picture-probe.ts's probeScript for the same sizes.
    let pictures = probe_pictures();
    let script = probe_script(&pictures[..1], "Picture Probe 2 KB");
    let size = plain_size(pictures[0].bytes.len());
    assert_eq!(
        script,
        format!(
            "Title: Picture Probe 2 KB\nAuthor: Marlow Penn\n\nINT. TEST ROOM - DAY\n\n\
             A plain room, kept for one question: whether this Kindle shows the picture in it.\n\n\
             The picture is a black frame around a grey checkerboard with a patch of static in it. \
             The bigger the file, the more of it is static.\n\n\
             The next picture is about {size}.\n\n[[img: p001-01.png w=60]]\n\nThe probe ends here.\n"
        )
    );
    assert!(probe_script(&pictures, "Picture Probe").contains("[[img: p003-01.png w=100 full]]"));
    assert!(probe_script(&pictures, "Picture Probe").contains("It has a page to itself."));
}
