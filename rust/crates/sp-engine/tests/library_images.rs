//! Port of tests/library-images.test.ts: the picture folder beside a
//! `.fountain`.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::png::{encode_png, png};
use common::{Scratch, names, write};
use sp_engine::fsx::{temp_path_for, write_atomic};
use sp_engine::library_images::shared::{IMAGE_MAX_EDGE, MAX_IMAGES};
use sp_engine::library_images::{
    ImagesRead, MAX_IMAGE_FILE_BYTES, image_folder_warnings, images_dir_for, is_image_temp,
    prune_images, read_images, write_images,
};
use sp_engine::nodepath::host;

struct Case {
    _s: Scratch,
    fountain: String,
}

/// A `.fountain` with a folder of the given files beside it (`None`: no
/// folder).
fn cached(files: Option<Vec<(String, Vec<u8>)>>) -> Case {
    let s = Scratch::new("library-images");
    let fountain = s.join("a.fountain");
    write(&fountain, "Title: T\n");
    if let Some(files) = files {
        let dir = s.dir("a.images");
        for (name, bytes) in files {
            std::fs::write(host::join(&[dir.as_str(), &name]), bytes).unwrap();
        }
    }
    Case { _s: s, fountain }
}

fn files(pairs: &[(&str, Vec<u8>)]) -> Option<Vec<(String, Vec<u8>)>> {
    Some(pairs.iter().map(|(n, b)| ((*n).to_owned(), b.clone())).collect())
}

fn many(n: usize) -> Vec<(String, Vec<u8>)> {
    (1..=n).map(|i| (format!("p{i:03}-01.png"), png(0))).collect()
}

fn none() -> BTreeSet<String> {
    BTreeSet::new()
}

fn asked(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

fn keys(read: &ImagesRead) -> Vec<String> {
    read.images.keys().cloned().collect()
}

/// A PNG header followed by padding, for the SIZE tests.
fn png_of_size(bytes: usize) -> Vec<u8> {
    let p = png(0);
    let mut out = vec![0u8; bytes];
    let n = bytes.min(p.len());
    out[..n].copy_from_slice(&p[..n]);
    out
}

/// A folder elsewhere holding one of the engine's own picture names, and a
/// `.fountain` whose `<stem>.images` is a symlink to it.
#[cfg(unix)]
fn linked_folder() -> (Case, Scratch, String, String) {
    let elsewhere_s = Scratch::new("elsewhere");
    let elsewhere = elsewhere_s.str();
    std::fs::write(host::join(&[elsewhere.as_str(), "p009-01.png"]), png(9)).unwrap();
    let case = cached(None);
    let dir = images_dir_for(&case.fountain);
    std::os::unix::fs::symlink(&elsewhere, &dir).unwrap();
    (case, elsewhere_s, dir, elsewhere)
}

// ---- imagesDirFor ----

#[cfg(unix)]
#[test]
fn images_dir_is_the_fountains_own_directory_and_stem() {
    assert_eq!(images_dir_for("/lib/Book.fountain"), "/lib/Book.images");
    assert_eq!(
        images_dir_for("/lib/sub/A Long Title.fountain"),
        "/lib/sub/A Long Title.images"
    );
}

#[cfg(unix)]
#[test]
fn images_dir_does_not_care_what_the_extension_was() {
    assert_eq!(images_dir_for("/lib/Book.txt"), "/lib/Book.images");
    assert_eq!(images_dir_for("/lib/Book"), "/lib/Book.images");
}

#[cfg(unix)]
#[test]
fn a_dotted_name_keeps_everything_but_the_last_extension() {
    assert_eq!(images_dir_for("/lib/Draft.v2.fountain"), "/lib/Draft.v2.images");
}

// ---- MAX_IMAGE_FILE_BYTES ----

#[test]
fn the_largest_picture_an_encoder_can_emit_fits_inside_it() {
    let edge = IMAGE_MAX_EDGE as usize;
    let px = edge * edge * 3;
    let mut noise = vec![0u8; px];
    let mut s: u32 = 12345;
    for b in &mut noise {
        s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
        *b = ((s >> 16) & 0xff) as u8;
    }
    let bytes = encode_png(&noise, IMAGE_MAX_EDGE as u32, IMAGE_MAX_EDGE as u32);
    assert!(bytes.len() > px);
    assert!(bytes.len() as u64 <= MAX_IMAGE_FILE_BYTES);
}

#[test]
fn and_it_is_not_so_loose_a_4_gb_file_would_pass() {
    assert!(MAX_IMAGE_FILE_BYTES < 16 * 1024 * 1024);
    assert_eq!(MAX_IMAGE_FILE_BYTES, 6_175_470);
}

// ---- isImageTemp ----

#[test]
fn recognizes_the_temp_name_the_writer_uses() {
    let s = Scratch::new("library-images");
    let name = host::basename(&temp_path_for(&s.join("p001-01.png")));
    assert!(is_image_temp(&name));
}

#[test]
fn a_write_that_fails_leaves_no_temp_behind() {
    let s = Scratch::new("library-images");
    let dir = s.dir("temp-fail");
    std::fs::create_dir(host::join(&[dir.as_str(), "p001-01.png"])).unwrap();
    assert!(write_atomic(&host::join(&[dir.as_str(), "p001-01.png"]), &png(0)).is_err());
    assert_eq!(names(&dir), vec!["p001-01.png"]);
}

#[test]
fn a_crashed_writes_temp_is_swept_and_only_that() {
    let s = Scratch::new("library-images");
    let dir = s.dir("temp-crash");
    write_atomic(&host::join(&[dir.as_str(), "p001-01.png"]), &png(0)).unwrap();
    std::fs::write(
        temp_path_for(&host::join(&[dir.as_str(), "p002-01.png"])),
        &png(0)[..10],
    )
    .unwrap();
    assert_eq!(names(&dir).len(), 2);
    assert_eq!(prune_images(&dir, &asked(&["p001-01.png"])), 1);
    assert_eq!(names(&dir), vec!["p001-01.png"]);
}

#[test]
fn a_temp_for_something_not_our_picture_is_not_ours() {
    assert!(!is_image_temp(".board-01.png.42.tmp"));
    assert!(!is_image_temp(".out.epub.42.tmp"));
    assert!(!is_image_temp(".notes.md.42.tmp"));
}

#[test]
fn a_plain_file_is_never_a_temp() {
    for name in ["p001-01.png", "p001-01.png.tmp", ".p001-01.png.tmp", "x.tmp"] {
        assert!(!is_image_temp(name), "{name}");
    }
}

// ---- readImages ----

#[test]
fn a_missing_folder_is_empty_and_blameless() {
    let c = cached(None);
    assert_eq!(read_images(&c.fountain, &none()), ImagesRead::default());
}

#[test]
fn an_empty_folder_likewise() {
    let c = cached(Some(vec![]));
    assert_eq!(read_images(&c.fountain, &none()), ImagesRead::default());
}

#[test]
fn reads_the_pictures_keyed_by_name() {
    let c = cached(files(&[("p001-01.png", png(4)), ("p002-01.png", png(9))]));
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p001-01.png", "p002-01.png"]);
    assert_eq!(read.images["p002-01.png"], png(9));
    assert_eq!(read.refused, 0);
}

#[test]
fn only_names_a_note_could_have_written_and_they_are_not_refusals() {
    let c = cached(files(&[
        ("p001-01.png", png(0)),
        ("a&b.png", png(0)),
        ("sneaky.png.txt", png(0)),
        ("notes.txt", png(0)),
        ("UPPER.PNG", png(0)),
    ]));
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p001-01.png"]);
    assert_eq!(read.refused, 0);
}

#[test]
fn a_file_bigger_than_we_could_have_written_is_refused_and_counted() {
    let c = cached(files(&[
        ("p001-01.png", png(0)),
        ("p002-01.png", png_of_size(MAX_IMAGE_FILE_BYTES as usize + 1)),
    ]));
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p001-01.png"]);
    assert_eq!(read.refused, 1);
}

#[test]
fn a_file_exactly_at_the_bound_is_still_ours() {
    let c = cached(files(&[(
        "p001-01.png",
        png_of_size(MAX_IMAGE_FILE_BYTES as usize),
    )]));
    let read = read_images(&c.fountain, &none());
    assert_eq!(read.images.len(), 1);
    assert_eq!(read.refused, 0);
}

#[test]
fn a_picture_that_is_not_one_is_refused_and_counted() {
    let c = cached(files(&[
        ("p001-01.png", png(0)),
        ("p002-01.png", vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46]),
        ("p003-01.png", vec![]),
        ("p004-01.png", png(0)[..28].to_vec()),
    ]));
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p001-01.png"]);
    assert_eq!(read.refused, 3);
    assert!(image_folder_warnings(&read)[0].contains("3 pictures could not be read"));
}

#[cfg(unix)]
#[test]
fn a_symlink_is_not_followed_whatever_it_points_at() {
    let c = cached(files(&[("p001-01.png", png(0))]));
    let outside_s = Scratch::new("outside");
    let outside = outside_s.join("outside.png");
    std::fs::write(&outside, png(7)).unwrap();
    std::os::unix::fs::symlink(
        &outside,
        host::join(&[images_dir_for(&c.fountain).as_str(), "p004-01.png"]),
    )
    .unwrap();
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p001-01.png"]);
    assert_eq!(read.refused, 1);
}

#[cfg(unix)]
#[test]
fn a_picture_folder_that_is_a_symlink_is_not_followed_and_says_so() {
    let (c, _e, _dir, _elsewhere) = linked_folder();
    let read = read_images(&c.fountain, &none());
    assert!(read.images.is_empty());
    assert!(read.folder_refused);
    let warnings = image_folder_warnings(&read);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("picture folder"));
    assert!(warnings[0].contains("not a folder"));
}

#[test]
fn so_is_a_file_where_the_folder_should_be() {
    let c = cached(None);
    std::fs::write(images_dir_for(&c.fountain), png(0)).unwrap();
    let read = read_images(&c.fountain, &none());
    assert!(read.images.is_empty());
    assert!(read.folder_refused);
    assert_eq!(image_folder_warnings(&read).len(), 1);
}

#[test]
fn a_directory_named_like_a_picture_is_refused() {
    let c = cached(files(&[("p001-01.png", png(0))]));
    std::fs::create_dir(host::join(&[
        images_dir_for(&c.fountain).as_str(),
        "p002-01.png",
    ]))
    .unwrap();
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p001-01.png"]);
    assert_eq!(read.refused, 1);
}

#[cfg(unix)]
#[test]
fn a_pipe_is_refused_without_ever_being_opened() {
    let c = cached(files(&[("p001-01.png", png(0))]));
    let made = std::process::Command::new("mkfifo")
        .arg(host::join(&[
            images_dir_for(&c.fountain).as_str(),
            "p002-01.png",
        ]))
        .status()
        .unwrap();
    assert!(made.success());
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p001-01.png"]);
    assert_eq!(read.refused, 1);
}

#[cfg(unix)]
#[test]
fn an_unreadable_file_costs_its_own_picture_and_no_other() {
    if common::is_root() {
        return;
    }
    let c = cached(files(&[("p001-01.png", png(0)), ("p002-01.png", png(0))]));
    let locked = host::join(&[images_dir_for(&c.fountain).as_str(), "p001-01.png"]);
    common::chmod(&locked, 0o000);
    let read = read_images(&c.fountain, &none());
    common::chmod(&locked, 0o644);
    assert_eq!(keys(&read), vec!["p002-01.png"]);
    assert_eq!(read.refused, 1);
}

#[test]
fn no_more_are_accepted_than_a_pdf_could_carry() {
    let c = cached(Some(many(MAX_IMAGES + 5)));
    let read = read_images(&c.fountain, &none());
    assert_eq!(read.images.len(), MAX_IMAGES);
    assert_eq!(keys(&read)[0], "p001-01.png");
    assert_eq!(
        keys(&read).last().unwrap(),
        &format!("p0{MAX_IMAGES}-01.png")
    );
}

#[test]
fn the_ones_the_cap_turned_away_are_counted() {
    let c = cached(Some(many(MAX_IMAGES + 5)));
    let read = read_images(&c.fountain, &none());
    assert_eq!(read.images.len(), MAX_IMAGES);
    assert_eq!(read.refused, 0);
    assert_eq!(read.over_cap, 5);
}

#[test]
fn a_name_no_note_could_carry_is_not_an_over_cap_loss() {
    let mut f: Vec<(String, Vec<u8>)> = (0..MAX_IMAGES + 5)
        .map(|i| (format!("a&b{i:03}.png"), png(0)))
        .collect();
    f.push(("p001-01.png".into(), png(0)));
    let c = cached(Some(f));
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p001-01.png"]);
    assert_eq!(read.refused, 0);
    assert_eq!(read.over_cap, 0);
}

#[test]
fn junk_cannot_occupy_a_place_in_that_cap() {
    let mut f: Vec<(String, Vec<u8>)> = vec![("p999-01.png".into(), png(0))];
    f.extend((0..MAX_IMAGES).map(|i| (format!("a&b{i:03}.png"), png(0))));
    let c = cached(Some(f));
    for i in 1..=MAX_IMAGES {
        std::fs::create_dir(host::join(&[
            images_dir_for(&c.fountain).as_str(),
            &format!("p{i:03}-01.png"),
        ]))
        .unwrap();
    }
    let read = read_images(&c.fountain, &none());
    assert_eq!(keys(&read), vec!["p999-01.png"]);
}

// ---- readImages, told which pictures the script asks for ----

#[test]
fn a_picture_the_folder_does_not_list_is_not_found() {
    let c = cached(files(&[("p001-01.png", png(0)), (".p002-01.png.icloud", png(0))]));
    let read = read_images(&c.fountain, &asked(&["p001-01.png", "p002-01.png"]));
    assert_eq!(keys(&read), vec!["p001-01.png"]);
    assert_eq!((read.refused, read.over_cap, read.not_found), (0, 0, 1));
}

#[test]
fn a_folder_that_is_not_there_holds_none_of_them() {
    let c = cached(None);
    assert_eq!(
        read_images(&c.fountain, &asked(&["p001-01.png", "p002-01.png"])).not_found,
        2
    );
}

#[test]
fn a_listed_picture_refused_is_not_also_not_found() {
    let c = cached(files(&[("p001-01.png", vec![])]));
    let read = read_images(&c.fountain, &asked(&["p001-01.png", "p002-01.png"]));
    assert_eq!((read.refused, read.not_found), (1, 1));
}

#[test]
fn a_listed_picture_past_the_cap_is_not_also_not_found() {
    let c = cached(Some(many(MAX_IMAGES + 1)));
    let last = format!("p0{}-01.png", MAX_IMAGES + 1);
    let read = read_images(&c.fountain, &asked(&["p001-01.png", &last]));
    assert!(!read.images.contains_key(&last));
    assert_eq!((read.over_cap, read.not_found), (1, 0));
}

#[cfg(unix)]
#[test]
fn a_folder_that_is_a_link_says_so_once_and_counts_nothing_not_found() {
    let (c, _e, _dir, _elsewhere) = linked_folder();
    let read = read_images(&c.fountain, &asked(&["p009-01.png", "p010-01.png"]));
    assert!(read.folder_refused);
    assert_eq!(read.not_found, 0);
    assert_eq!(image_folder_warnings(&read).len(), 1);
}

#[test]
fn asked_for_nothing_nothing_is_missing() {
    assert_eq!(read_images(&cached(None).fountain, &none()).not_found, 0);
    assert_eq!(
        read_images(&cached(files(&[("p001-01.png", png(0))])).fountain, &none()).not_found,
        0
    );
}

#[test]
fn a_stray_file_is_not_a_refusal_when_the_script_says_what_it_shows() {
    let c = cached(files(&[
        ("p001-01.png", png(1)),
        ("p002-01.png", png(2)),
        ("stray.png", vec![]),
    ]));
    let read = read_images(&c.fountain, &asked(&["p001-01.png", "p002-01.png"]));
    assert_eq!(keys(&read), vec!["p001-01.png", "p002-01.png"]);
    assert_eq!((read.refused, read.over_cap, read.not_found), (0, 0, 0));
    assert!(image_folder_warnings(&read).is_empty());
}

#[test]
fn nor_is_a_stray_past_the_cap() {
    let mut f = many(MAX_IMAGES);
    f.push(("x-stray-1.png".into(), png(0)));
    f.push(("x-stray-2.png".into(), png(0)));
    let c = cached(Some(f));
    let read = read_images(&c.fountain, &asked(&["p001-01.png"]));
    assert!(read.images.contains_key("p001-01.png"));
    assert_eq!((read.refused, read.over_cap), (0, 0));
}

#[test]
fn a_picture_the_script_shows_is_still_counted_beside_a_stray() {
    let c = cached(files(&[("p001-01.png", vec![]), ("stray.png", vec![])]));
    assert_eq!(
        read_images(&c.fountain, &asked(&["p001-01.png"])).refused,
        1
    );
}

#[test]
fn asked_for_nothing_every_refused_file_counts() {
    let c = cached(files(&[("p001-01.png", png(0)), ("stray.png", vec![])]));
    assert_eq!(read_images(&c.fountain, &none()).refused, 1);
    let mut f = many(MAX_IMAGES);
    f.push(("x-stray-1.png".into(), png(0)));
    assert_eq!(read_images(&cached(Some(f)).fountain, &none()).over_cap, 1);
}

// ---- imageFolderWarnings ----

fn counts(kept: usize, refused: usize, over_cap: usize, not_found: usize) -> ImagesRead {
    ImagesRead {
        images: (0..kept)
            .map(|i| (format!("p{i}.png"), png(0)))
            .collect::<BTreeMap<_, _>>(),
        refused,
        over_cap,
        not_found,
        folder_refused: false,
    }
}

#[test]
fn silence_when_the_folder_gave_up_everything() {
    assert!(image_folder_warnings(&counts(3, 0, 0, 0)).is_empty());
}

#[test]
fn one_refusal_is_singular_and_says_where_to_look() {
    let w = &image_folder_warnings(&counts(0, 1, 0, 0))[0];
    assert!(w.starts_with("1 picture ") && !w.starts_with("1 pictures"));
    assert!(w.contains("picture folder"));
    assert!(!w.contains('—'));
    assert!(w.ends_with('.'));
}

#[test]
fn more_than_one_is_plural() {
    assert!(image_folder_warnings(&counts(0, 4, 0, 0))[0].starts_with("4 pictures "));
}

#[test]
fn a_folder_holding_more_than_the_book_can_carry_says_so() {
    let w = &image_folder_warnings(&counts(MAX_IMAGES, 0, 5, 0))[0];
    assert!(w.contains(&format!("{} pictures", MAX_IMAGES + 5)));
    assert!(w.contains(&format!("first {MAX_IMAGES}")));
    assert!(w.contains("picture folder"));
    assert!(!w.contains('—'));
}

#[test]
fn the_two_losses_are_separate_sentences() {
    let out = image_folder_warnings(&counts(MAX_IMAGES, 2, 3, 0));
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|w| w.contains("could not be read")));
    assert!(out.iter().any(|w| w.contains(&format!("{} pictures", MAX_IMAGES + 3))));
}

#[test]
fn pictures_not_found_are_said_with_the_count() {
    assert_eq!(
        image_folder_warnings(&counts(0, 0, 0, 1)),
        vec!["1 picture this script shows could not be found in its picture folder, so the book was made without it."]
    );
    assert_eq!(
        image_folder_warnings(&counts(0, 0, 0, 2)),
        vec!["2 pictures this script shows could not be found in its picture folder, so the book was made without them."]
    );
}

#[test]
fn not_found_and_not_read_are_separate_sentences() {
    let out = image_folder_warnings(&counts(0, 1, 0, 1));
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|w| w.starts_with("1 picture could not be read")));
    assert!(
        out.iter()
            .any(|w| w.starts_with("1 picture this script shows could not be found"))
    );
    assert!(out.iter().all(|w| !w.contains('—')));
}

// ---- pruneImages ----

#[test]
fn prune_removes_stale_pictures_and_abandoned_temps() {
    let c = cached(files(&[
        ("p001-01.png", png(0)),
        ("p009-09.png", png(0)),
        (".p002-01.png.999.tmp", png(0)),
    ]));
    let dir = images_dir_for(&c.fountain);
    assert_eq!(prune_images(&dir, &asked(&["p001-01.png"])), 2);
    assert_eq!(names(&dir), vec!["p001-01.png"]);
}

#[test]
fn prune_never_removes_a_picture_a_person_put_there() {
    let kept = ["board-01.png", "sunset.png", "p1.png", "A-01.png", "readme.md"];
    let c = cached(Some(kept.iter().map(|n| ((*n).to_owned(), png(0))).collect()));
    let dir = images_dir_for(&c.fountain);
    assert_eq!(prune_images(&dir, &none()), 0);
    let mut want: Vec<String> = kept.iter().map(|s| (*s).to_owned()).collect();
    want.sort();
    assert_eq!(names(&dir), want);
}

#[test]
fn prune_takes_the_folder_away_when_it_emptied_it() {
    let c = cached(files(&[("p001-01.png", png(0))]));
    let dir = images_dir_for(&c.fountain);
    assert_eq!(prune_images(&dir, &none()), 1);
    assert!(!std::path::Path::new(&dir).exists());
}

#[test]
fn prune_leaves_a_folder_it_did_not_empty_and_one_it_did_not_touch() {
    let a = cached(files(&[("p001-01.png", png(0)), ("readme.md", png(0))]));
    let dir = images_dir_for(&a.fountain);
    assert_eq!(prune_images(&dir, &none()), 1);
    assert_eq!(names(&dir), vec!["readme.md"]);
    let b = cached(Some(vec![]));
    let dir = images_dir_for(&b.fountain);
    assert_eq!(prune_images(&dir, &none()), 0);
    assert!(names(&dir).is_empty());
}

#[test]
fn prune_on_a_missing_folder_is_not_an_error_and_creates_nothing() {
    let c = cached(None);
    let dir = images_dir_for(&c.fountain);
    assert_eq!(prune_images(&dir, &none()), 0);
    assert!(!std::path::Path::new(&dir).exists());
}

#[cfg(unix)]
#[test]
fn prune_never_deletes_through_a_symlinked_folder() {
    let (_c, _e, dir, elsewhere) = linked_folder();
    assert_eq!(prune_images(&dir, &none()), 0);
    assert_eq!(names(&elsewhere), vec!["p009-01.png"]);
}

// ---- writeImages ----

#[test]
fn write_images_writes_and_leaves_no_temp() {
    let c = cached(None);
    let dir = images_dir_for(&c.fountain);
    write_images(
        &dir,
        &[("p001-01.png".into(), png(4)), ("p002-01.png".into(), png(5))],
    )
    .unwrap();
    assert_eq!(names(&dir), vec!["p001-01.png", "p002-01.png"]);
}

#[test]
fn write_images_adopts_a_folder_without_taking_the_users_files() {
    let c = cached(files(&[("board-01.png", png(0)), ("p009-09.png", png(0))]));
    let dir = images_dir_for(&c.fountain);
    write_images(&dir, &[("p001-01.png".into(), png(0))]).unwrap();
    assert_eq!(names(&dir), vec!["board-01.png", "p001-01.png"]);
}

#[test]
fn write_images_refuses_a_path_taken_by_a_file() {
    let c = cached(None);
    std::fs::write(images_dir_for(&c.fountain), "a file, not a folder").unwrap();
    assert!(write_images(&images_dir_for(&c.fountain), &[("p001-01.png".into(), png(0))]).is_err());
}

#[cfg(unix)]
#[test]
fn write_images_refuses_a_symlinked_folder() {
    let (_c, _e, dir, elsewhere) = linked_folder();
    let err = write_images(&dir, &[("p001-01.png".into(), png(0))]).unwrap_err();
    assert_eq!(err.fs_code(), Some("ENOTDIR"));
    assert_eq!(names(&elsewhere), vec!["p009-01.png"]);
}

#[test]
fn write_images_makes_the_folder_and_never_its_parents() {
    let s = Scratch::new("library-images");
    let base = s.dir("base");
    let dir = host::join(&[base.as_str(), "missing", "deeper", "x.images"]);
    let err = write_images(&dir, &[("p001-01.png".into(), png(0))]).unwrap_err();
    assert_eq!(err.fs_code(), Some("ENOENT"));
    assert!(names(&base).is_empty());
}
