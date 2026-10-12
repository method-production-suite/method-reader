//! Port of tests/bundle-archive.test.ts (27 checks): opening a .deb and an
//! .rpm with no outside tool. The archives "built by the real tools" are the
//! same frozen bytes the TypeScript test carries (GNU ar 2.46, GNU cpio
//! 2.15, GNU tar 1.35, bsdtar), in tests/data/real-archives.json.

mod packaging_common;

use std::fs;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use packaging_common::*;
use xtask::packaging::archive::{
    ArchiveEntry, ar_members, bundle_entries, cpio_entries, deb_entries, find_entry, gunzip,
    rpm_entries, rpm_payload, tar_entries,
};

fn real(name: &str) -> Vec<u8> {
    let json: serde_json::Value =
        serde_json::from_str(include_str!("data/real-archives.json")).unwrap();
    STANDARD.decode(json[name].as_str().unwrap()).unwrap()
}

fn names(entries: &[ArchiveEntry]) -> Vec<String> {
    let mut v: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    v.sort();
    v
}

fn member<'a>(m: &'a [(String, Vec<u8>)], name: &str) -> &'a [u8] {
    &m.iter().find(|(n, _)| n == name).unwrap().1
}

fn keys(m: &[(String, Vec<u8>)]) -> Vec<&str> {
    m.iter().map(|(n, _)| n.as_str()).collect()
}

// ---- the ar container a .deb is ----

#[test]
fn names_its_three_members_and_hands_back_their_exact_bytes() {
    let deb = concat(&[
        b"!<arch>\n",
        &ar_member("debian-binary/  ", b"2.0\n"),
        &ar_member("control.tar.gz/ ", b"CONTROL"),
        &ar_member("data.tar.gz/    ", b"DATA"),
    ]);
    let m = ar_members(&deb).unwrap();
    assert_eq!(keys(&m), ["debian-binary", "control.tar.gz", "data.tar.gz"]);
    assert_eq!(member(&m, "data.tar.gz"), b"DATA");
}

#[test]
fn an_odd_sized_member_does_not_shift_the_one_after_it() {
    let deb = concat(&[
        b"!<arch>\n",
        &ar_member("odd/            ", b"abc"),
        &ar_member("after/          ", b"TAIL"),
    ]);
    let m = ar_members(&deb).unwrap();
    assert_eq!(keys(&m), ["odd", "after"]);
    assert_eq!(member(&m, "after"), b"TAIL");
}

#[test]
fn gnu_ar_built_the_same_shape_odd_member_and_all() {
    let m = ar_members(&real("ar")).unwrap();
    assert_eq!(keys(&m), ["debian-binary", "odd.txt", "after.txt"]);
    assert_eq!(member(&m, "debian-binary"), b"2.0\n");
    assert_eq!(member(&m, "odd.txt"), b"abc");
    assert_eq!(member(&m, "after.txt"), b"TAIL");
}

#[test]
fn a_15_byte_name_fills_the_field_exactly_and_a_longer_one_comes_out_of_the_long_name_table() {
    let m = ar_members(&real("arBoundary")).unwrap();
    assert_eq!(keys(&m), ["abcdefghijklmno", "abcdefghijklmnop"]);
    assert_eq!(member(&m, "abcdefghijklmno"), b"FIFTEEN");
    assert_eq!(member(&m, "abcdefghijklmnop"), b"SIXTEEN");
}

#[test]
fn it_refuses_something_that_is_not_an_ar_archive_at_all() {
    err_contains(ar_members(b"not an archive"), "!<arch>");
}

#[test]
fn a_member_that_runs_off_the_end_of_the_file_is_an_error_not_short_bytes() {
    let whole = concat(&[b"!<arch>\n", &ar_member("data.tar.gz/    ", b"DATA")]);
    err_contains(ar_members(&whole[..8 + 60 + 2]), "truncat");
}

#[test]
fn deb_entries_reads_a_whole_deb_off_disk() {
    let s = Scratch::new("archive");
    let path = s.join("fake.deb");
    fake_deb(&path, "usr/bin/screepub-engine", b"ENGINE");
    let entries = deb_entries(&path).unwrap();
    assert_eq!(names(&entries), ["usr/bin/screepub-engine"]);
    assert_eq!(entries[0].data, b"ENGINE");
    assert_ne!(entries[0].mode & 0o111, 0);
}

#[test]
fn deb_entries_reads_a_deb_whose_members_gnu_ar_and_gnu_tar_wrote() {
    let s = Scratch::new("archive");
    let path = s.join("real.deb");
    fs::write(&path, real("deb")).unwrap();
    let entries = bundle_entries(&path).unwrap();
    assert_eq!(
        names(&entries),
        [
            "./usr/bin/screepub-engine",
            "./usr/lib/Screepub/LICENSE",
            "./usr/lib/Screepub/odd3"
        ]
    );
    let engine = find_entry(&entries, "usr/bin/screepub-engine").unwrap();
    assert_eq!(engine.data, b"ENGINE");
    assert_ne!(engine.mode & 0o111, 0);
    assert_eq!(
        find_entry(&entries, "usr/lib/Screepub/LICENSE")
            .unwrap()
            .mode
            & 0o111,
        0
    );
    assert_eq!(
        find_entry(&entries, "usr/lib/Screepub/odd3").unwrap().data,
        b"XY"
    );
}

#[test]
fn a_deb_with_no_data_tar_gz_says_so_instead_of_returning_nothing() {
    let s = Scratch::new("archive");
    let path = s.join("empty.deb");
    fs::write(
        &path,
        concat(&[b"!<arch>\n", &ar_member("debian-binary/  ", b"2.0\n")]),
    )
    .unwrap();
    err_contains(deb_entries(&path), "data.tar.gz");
}

// ---- the cpio payload an .rpm carries ----

#[test]
fn it_reads_names_modes_and_bytes_and_stops_at_the_trailer() {
    let cpio = concat(&[
        &cpio_member("./usr/bin/screepub-engine", b"ENGINE", 0o100755),
        &cpio_member("./usr/lib/Screepub/LICENSE", b"AGPL", 0o100644),
        &cpio_trailer(),
    ]);
    let entries = cpio_entries(&cpio).unwrap();
    let in_order: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        in_order,
        ["./usr/bin/screepub-engine", "./usr/lib/Screepub/LICENSE"]
    );
    assert_eq!(entries[0].data, b"ENGINE");
    assert_ne!(entries[0].mode & 0o111, 0);
    assert_eq!(entries[1].mode & 0o111, 0);
}

#[test]
fn a_name_whose_length_is_not_a_multiple_of_four_does_not_shift_the_data() {
    for name in ["./a", "./abcd", "./abcde"] {
        let entries = cpio_entries(&concat(&[
            &cpio_member(name, b"XY", 0o100755),
            &cpio_trailer(),
        ]))
        .unwrap();
        assert_eq!(names(&entries), [name]);
        assert_eq!(entries[0].data, b"XY");
    }
}

#[test]
fn gnu_cpio_wrote_directories_among_the_files_and_only_the_files_come_back() {
    let entries = cpio_entries(&gunzip(&real("cpioGnuGz")).unwrap()).unwrap();
    assert_eq!(
        names(&entries),
        [
            "usr/bin/screepub-engine",
            "usr/lib/Screepub/LICENSE",
            "usr/lib/Screepub/odd3"
        ]
    );
    let engine = find_entry(&entries, "usr/bin/screepub-engine").unwrap();
    assert_eq!(engine.data, b"ENGINE");
    assert_ne!(engine.mode & 0o111, 0);
    assert_eq!(
        find_entry(&entries, "usr/lib/Screepub/LICENSE")
            .unwrap()
            .mode
            & 0o111,
        0
    );
    let odd = find_entry(&entries, "usr/lib/Screepub/odd3").unwrap();
    assert_eq!(odd.data, b"XY");
    assert_eq!(odd.size, 2);
}

#[test]
fn bsdtar_wrote_the_same_tree_with_the_dot_slash_prefix_and_that_reads_too() {
    let entries = cpio_entries(&gunzip(&real("cpioBsdGz")).unwrap()).unwrap();
    assert_eq!(
        names(&entries),
        [
            "./usr/bin/screepub-engine",
            "./usr/lib/Screepub/LICENSE",
            "./usr/lib/Screepub/odd3"
        ]
    );
    assert_eq!(
        find_entry(&entries, "usr/lib/Screepub/odd3").unwrap().data,
        b"XY"
    );
    assert_eq!(
        find_entry(&entries, "./usr/lib/Screepub/LICENSE")
            .unwrap()
            .data,
        b"AGPL"
    );
}

#[test]
fn it_refuses_a_payload_that_is_not_cpio() {
    err_contains(
        cpio_entries(b"nope nope nope nope nope nope nope nope nope nope nope nope"),
        "07070",
    );
}

#[test]
fn a_cpio_that_just_stops_with_no_trailer_is_an_error() {
    err_contains(
        cpio_entries(&cpio_member(
            "./usr/bin/screepub-engine",
            b"ENGINE",
            0o100755,
        )),
        "TRAILER",
    );
}

#[test]
fn rpm_payload_skips_the_lead_and_both_headers_padding_included() {
    assert_eq!(
        rpm_payload(&fake_rpm(b"PAYLOAD-BYTES")).unwrap(),
        b"PAYLOAD-BYTES"
    );
}

#[test]
fn rpm_payload_refuses_a_file_that_is_not_an_rpm() {
    err_contains(rpm_payload(&[0u8; 200]), "rpm");
}

#[test]
fn rpm_entries_reads_a_whole_rpm_off_disk() {
    let s = Scratch::new("archive");
    let path = s.join("fake.rpm");
    let cpio = concat(&[
        &cpio_member("./usr/bin/screepub-engine", b"ENGINE", 0o100755),
        &cpio_trailer(),
    ]);
    fs::write(&path, fake_rpm(&gzip(&cpio))).unwrap();
    let entries = rpm_entries(&path).unwrap();
    assert_eq!(names(&entries), ["./usr/bin/screepub-engine"]);
    assert_eq!(entries[0].data, b"ENGINE");
}

#[test]
fn rpm_entries_reads_a_payload_a_real_tool_wrote() {
    let s = Scratch::new("archive");
    let path = s.join("real-payload.rpm");
    fs::write(&path, fake_rpm(&real("cpioBsdGz"))).unwrap();
    assert_eq!(
        names(&bundle_entries(&path).unwrap()),
        [
            "./usr/bin/screepub-engine",
            "./usr/lib/Screepub/LICENSE",
            "./usr/lib/Screepub/odd3"
        ]
    );
}

#[test]
fn a_payload_compressed_with_something_other_than_gzip_names_the_problem() {
    let s = Scratch::new("archive");
    let path = s.join("zstd.rpm");
    fs::write(&path, fake_rpm(b"(\xb5/\xfd not gzip at all")).unwrap();
    err_contains(rpm_entries(&path), "PAYLOADCOMPRESSOR");
}

// ---- finding one file inside either container ----

fn two() -> Vec<ArchiveEntry> {
    vec![
        ArchiveEntry {
            name: "usr/bin/screepub-desktop".into(),
            size: 3,
            mode: 0o755,
            data: b"abc".to_vec(),
        },
        ArchiveEntry {
            name: "./usr/bin/screepub-engine".into(),
            size: 6,
            mode: 0o755,
            data: b"ENGINE".to_vec(),
        },
    ]
}

fn one(name: &str) -> Vec<ArchiveEntry> {
    vec![ArchiveEntry {
        name: name.into(),
        size: 1,
        mode: 0,
        data: b"x".to_vec(),
    }]
}

#[test]
fn it_matches_across_the_dot_slash_prefix_the_two_formats_disagree_about() {
    let e = two();
    assert_eq!(find_entry(&e, "usr/bin/screepub-engine").unwrap().size, 6);
    assert_eq!(
        find_entry(&e, "./usr/bin/screepub-desktop").unwrap().size,
        3
    );
}

#[test]
fn it_does_not_match_a_longer_name_that_merely_ends_the_same_way() {
    err_contains(
        find_entry(
            &one("usr/bin/not-screepub-engine"),
            "usr/bin/screepub-engine",
        )
        .map(|_| ()),
        "not-screepub-engine",
    );
}

#[test]
fn nor_a_name_that_merely_starts_the_same_way() {
    err_contains(
        find_entry(
            &one("./usr/bin/screepub-engine.bak"),
            "usr/bin/screepub-engine",
        )
        .map(|_| ()),
        "engine.bak",
    );
}

#[test]
fn a_miss_names_what_it_did_find_so_the_failure_is_diagnosable() {
    err_contains(
        find_entry(&two(), "usr/bin/nothing").map(|_| ()),
        "screepub-desktop",
    );
}

// ---- bundle_entries dispatches on the extension ----

#[test]
fn it_refuses_a_container_it_has_no_reader_for() {
    err_contains(bundle_entries(Path::new("/tmp/Screepub.dmg")), ".deb");
    err_contains(bundle_entries(Path::new("/tmp/Screepub-setup.exe")), ".rpm");
}

#[test]
fn tar_entries_still_walks_a_plain_tar_which_both_readers_lean_on() {
    let tar = concat(&[
        &tar_member("a", b"one", 0o755),
        &tar_member("b", b"two", 0o755),
        &[0u8; 1024],
    ]);
    let in_order: Vec<String> = tar_entries(&tar).into_iter().map(|e| e.name).collect();
    assert_eq!(in_order, ["a", "b"]);
}

#[test]
fn tar_entries_returns_only_regular_files_with_a_directory_in_the_way() {
    let mut dir = vec![0u8; 512];
    dir[..8].copy_from_slice(b"usr/bin/");
    dir[100..108].copy_from_slice(b"0000755\0");
    dir[124..136].copy_from_slice(b"00000000000\0");
    dir[156] = b'5';
    let tar = concat(&[
        &dir,
        &tar_member("usr/bin/screepub-engine", b"ENGINE", 0o755),
        &[0u8; 1024],
    ]);
    assert_eq!(names(&tar_entries(&tar)), ["usr/bin/screepub-engine"]);
}

#[test]
fn the_command_lists_a_package_and_refuses_what_it_cannot_open() {
    let s = Scratch::new("archive-cli");
    let path = s.join("real.deb");
    fs::write(&path, real("deb")).unwrap();
    let (code, out, _) = xtask(&["bundle-archive", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("0755"), "{out}");
    assert!(out.contains("./usr/bin/screepub-engine"), "{out}");
    let (code, _, err) = xtask(&["bundle-archive", "/tmp/Screepub.dmg"]);
    assert_eq!(code, 1);
    assert!(err.contains("neither a .deb nor an .rpm"), "{err}");
    assert_eq!(xtask(&["bundle-archive"]).0, 2);
}
