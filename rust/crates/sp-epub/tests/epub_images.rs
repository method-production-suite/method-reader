//! Port of tests/epub-images.test.ts: the pictures a book carries (registry
//! #19), which go into the container, how they are stored, how the package
//! document lists them, the cover (#19b), the preview's inlined pictures,
//! and epubcheck's verdict.
//!
//! Where the TypeScript converts images.pdf or torture.pdf, this reads the
//! oracle's dump of the same conversion (rust/oracle/<fixture>/): its
//! tokens, its title and its picture folder.

mod common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use common::{body_of, entry_text, matches, methods, read_zip, repo_root, tokens};
use sp_epub::{
    BodyOptions, BookBody, BookImage, BookMeta, EpubError, build_epub, content_identifier,
    image_ids, screenplay_css, tokens_to_body, tokens_to_preview_html,
};
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, Token, resolve_format_options};

const EPOCH: i64 = 1_767_225_600;

/// Enough of a PNG for everything but epubcheck: the signature, then bytes
/// that tell one picture from another.
fn png(tail: &[u8]) -> Vec<u8> {
    let mut v = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    v.extend_from_slice(tail);
    v
}

fn image(name: &str, bytes: Vec<u8>) -> BookImage {
    BookImage {
        name: name.to_owned(),
        bytes,
    }
}

fn meta() -> BookMeta {
    BookMeta {
        title: "T".to_owned(),
        identifier: Some("urn:screepub:test".to_owned()),
        ..BookMeta::default()
    }
}

fn opts(json: serde_json::Value) -> FormatOptions {
    resolve_format_options(Some(&json), &DEFAULT_FORMAT_OPTIONS)
}

/// A body that shows each named picture once, in order.
fn body_showing(names: &[&str], format: &FormatOptions) -> BookBody {
    let notes: String = names
        .iter()
        .map(|n| format!("[[img: {n} w=50]]\n\n"))
        .collect();
    body_of(
        &format!("INT. LAB - DAY\n\nBefore.\n\n{notes}After.\n"),
        format,
        names,
    )
}

fn build(body: &BookBody, format: &FormatOptions, images: &[BookImage]) -> Vec<u8> {
    build_epub(&meta(), body, format, images, EPOCH).expect("a book")
}

/// Files only: the zip has a folder entry for each directory a path implies.
fn image_entries(bytes: &[u8]) -> Vec<String> {
    read_zip(bytes)
        .into_iter()
        .filter(|e| e.name.starts_with("OEBPS/images/") && !e.dir)
        .map(|e| e.name)
        .collect()
}

fn image_items(opf: &str) -> Vec<String> {
    regex::Regex::new(r#"<item [^>]*media-type="image/png"[^>]*/>"#)
        .unwrap()
        .find_iter(opf)
        .map(|m| m.as_str().to_owned())
        .collect()
}

fn entry_bytes(bytes: &[u8], name: &str) -> Vec<u8> {
    read_zip(bytes)
        .into_iter()
        .find(|e| e.name == name)
        .unwrap()
        .data
}

fn opf(bytes: &[u8]) -> String {
    entry_text(bytes, "OEBPS/package.opf").unwrap()
}

fn spine(opf: &str) -> String {
    regex::Regex::new(r"<spine>([\s\S]*)</spine>")
        .unwrap()
        .captures(opf)
        .unwrap()[1]
        .to_owned()
}

fn plain_body(format: &FormatOptions) -> BookBody {
    body_of("INT. LAB - DAY\n\nBefore.\n", format, &[])
}

mod the_pictures_a_book_carries {
    use super::*;

    #[test]
    fn a_picture_the_body_shows_ships_under_its_own_name_bytes_untouched_with_its_manifest_item() {
        let bytes = png(&[1, 2, 3]);
        let epub = build(
            &body_showing(&["p001-01.png"], &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[image("p001-01.png", bytes.clone())],
        );
        assert_eq!(image_entries(&epub), ["OEBPS/images/p001-01.png"]);
        assert_eq!(entry_bytes(&epub, "OEBPS/images/p001-01.png"), bytes);
        assert_eq!(
            image_items(&opf(&epub)),
            ["<item id=\"img-p001-01\" href=\"images/p001-01.png\" media-type=\"image/png\"/>"]
        );
    }

    #[test]
    fn a_picture_the_body_does_not_show_is_not_shipped_however_it_was_passed() {
        let epub = build(
            &body_showing(&["p001-01.png"], &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[
                image("p009-01.png", png(&[9])),
                image("p001-01.png", png(&[1])),
            ],
        );
        assert_eq!(image_entries(&epub), ["OEBPS/images/p001-01.png"]);
        assert!(!opf(&epub).contains("p009-01"));
    }

    #[test]
    fn a_body_that_shows_no_picture_gets_no_picture_whatever_the_caller_holds() {
        let epub = build(
            &plain_body(&DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[image("p001-01.png", png(&[1]))],
        );
        assert!(image_entries(&epub).is_empty());
        assert!(image_items(&opf(&epub)).is_empty());
    }

    #[test]
    fn a_book_without_pictures_is_the_book_it_was_same_entries_same_bytes_same_manifest() {
        let plain = plain_body(&DEFAULT_FORMAT_OPTIONS);
        let a = read_zip(&build(&plain, &DEFAULT_FORMAT_OPTIONS, &[]));
        let b = read_zip(&build(
            &plain,
            &DEFAULT_FORMAT_OPTIONS,
            &[image("p001-01.png", png(&[1]))],
        ));
        // One clock, so the modified date is the same and nothing is
        // stripped before comparing.
        assert_eq!(a, b);
        let opf = String::from_utf8(
            a.iter()
                .find(|e| e.name == "OEBPS/package.opf")
                .unwrap()
                .data
                .clone(),
        )
        .unwrap();
        assert!(opf.contains(
            "  <manifest>\n    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n    <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n    <item id=\"titlepage\" href=\"titlepage.xhtml\" media-type=\"application/xhtml+xml\"/>\n    <item id=\"body001\" href=\"text/body001.xhtml\" media-type=\"application/xhtml+xml\"/>\n  </manifest>\n"
        ));
    }

    #[test]
    fn a_name_passed_twice_ships_once_the_first_bytes_with_it() {
        let epub = build(
            &body_showing(&["p001-01.png"], &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[
                image("p001-01.png", png(&[1])),
                image("p001-01.png", png(&[2])),
            ],
        );
        assert_eq!(image_items(&opf(&epub)).len(), 1);
        assert_eq!(
            methods(&epub)
                .iter()
                .filter(|(n, _)| n == "OEBPS/images/p001-01.png")
                .count(),
            1
        );
        assert_eq!(entry_bytes(&epub, "OEBPS/images/p001-01.png"), png(&[1]));
    }

    #[test]
    fn a_picture_the_body_shows_but_nobody_handed_over_is_a_caller_bug_and_says_so() {
        let err = build_epub(
            &meta(),
            &body_showing(&["p001-01.png", "p002-01.png"], &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[image("p001-01.png", png(&[1]))],
            EPOCH,
        )
        .unwrap_err();
        assert!(err.to_string().contains("p002-01.png"));
        let err = build_epub(
            &meta(),
            &body_showing(&["p001-01.png"], &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[],
            EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            err,
            EpubError::MissingPictures(vec!["p001-01.png".to_owned()])
        );
        assert!(err.to_string().contains("p001-01.png"));
    }

    #[test]
    fn pictures_are_stored_not_deflated_and_everything_else_still_deflates() {
        let epub = build(
            &body_showing(&["p001-01.png", "p002-01.png"], &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[
                image("p001-01.png", png(&[1])),
                image("p002-01.png", png(&[2])),
            ],
        );
        let m: std::collections::HashMap<String, u16> = methods(&epub).into_iter().collect();
        assert_eq!(m["OEBPS/images/p001-01.png"], 0);
        assert_eq!(m["OEBPS/images/p002-01.png"], 0);
        assert_eq!(m["mimetype"], 0);
        assert_eq!(m["OEBPS/text/body001.xhtml"], 8);
        assert_eq!(m["OEBPS/package.opf"], 8);
    }

    #[test]
    fn a_picture_is_a_resource_manifested_never_in_the_spine_no_properties() {
        let epub = build(
            &body_showing(&["p001-01.png"], &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[image("p001-01.png", png(&[1]))],
        );
        let opf = opf(&epub);
        assert_eq!(image_items(&opf).len(), 1);
        assert!(!spine(&opf).contains("img"));
        assert!(!image_items(&opf).concat().contains("properties="));
    }

    #[test]
    fn the_manifest_href_and_the_figure_src_name_the_same_zip_entry() {
        let epub = build(
            &body_showing(&["p001-01.png"], &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &[image("p001-01.png", png(&[1]))],
        );
        let href = regex::Regex::new(r#"<item id="img-[^"]*" href="([^"]+)""#)
            .unwrap()
            .captures(&opf(&epub))
            .unwrap()[1]
            .to_owned();
        let xhtml = entry_text(&epub, "OEBPS/text/body001.xhtml").unwrap();
        let src = regex::Regex::new(r#"<img src="([^"]+)""#)
            .unwrap()
            .captures(&xhtml)
            .unwrap()[1]
            .to_owned();
        // Resolved, not reasoned about.
        let resolve = |reference: &str, from: &str| {
            let mut parts: Vec<&str> = from.split('/').collect();
            parts.pop();
            for seg in reference.split('/') {
                match seg {
                    ".." => {
                        parts.pop();
                    }
                    "." => {}
                    s => parts.push(s),
                }
            }
            parts.join("/")
        };
        assert_eq!(
            resolve(&href, "OEBPS/package.opf"),
            "OEBPS/images/p001-01.png"
        );
        assert_eq!(
            resolve(&src, "OEBPS/text/body001.xhtml"),
            "OEBPS/images/p001-01.png"
        );
    }

    #[test]
    fn the_stylesheet_carries_the_figure_rules_exactly_when_the_book_ships_a_picture() {
        let format = opts(serde_json::json!({ "elementSpacingEm": 0.6 }));
        let shown = build(
            &body_showing(&["p001-01.png"], &format),
            &format,
            &[image("p001-01.png", png(&[1]))],
        );
        assert_eq!(
            entry_text(&shown, "OEBPS/style.css").unwrap(),
            screenplay_css(&format, true)
        );
        let none = build(
            &plain_body(&format),
            &format,
            &[image("p001-01.png", png(&[1]))],
        );
        assert_eq!(
            entry_text(&none, "OEBPS/style.css").unwrap(),
            screenplay_css(&format, false)
        );
    }
}

// ── the cover ────────────────────────────────────────────

mod the_cover_in_the_package {
    use super::*;

    const CARD: &str = "[[img: p001-01.png w=100 full]]\n\n";
    const SCRIPT: &str = "INT. LAB - DAY\n\nWork.\n";

    fn cover_body(src: &str, format: &FormatOptions, names: &[&str]) -> BookBody {
        body_of(src, format, names)
    }

    fn card_body() -> BookBody {
        cover_body(
            &format!("{CARD}{SCRIPT}"),
            &DEFAULT_FORMAT_OPTIONS,
            &["p001-01.png"],
        )
    }

    fn metadata_of(opf: &str) -> String {
        regex::Regex::new(r"<metadata[^>]*>([\s\S]*)</metadata>")
            .unwrap()
            .captures(opf)
            .unwrap()[1]
            .to_owned()
    }

    #[test]
    fn a_cover_no_page_draws_still_ships_its_bytes_its_manifest_item_and_both_declarations() {
        let bytes = png(&[7]);
        let body = card_body();
        assert_eq!(body.cover_image.as_deref(), Some("p001-01.png"));
        assert!(body.used_images.is_empty());
        let epub = build(
            &body,
            &DEFAULT_FORMAT_OPTIONS,
            &[image("p001-01.png", bytes.clone())],
        );
        assert_eq!(image_entries(&epub), ["OEBPS/images/p001-01.png"]);
        assert_eq!(entry_bytes(&epub, "OEBPS/images/p001-01.png"), bytes);
        let opf = opf(&epub);
        assert_eq!(
            image_items(&opf),
            [
                "<item id=\"img-p001-01\" href=\"images/p001-01.png\" media-type=\"image/png\" properties=\"cover-image\"/>"
            ]
        );
        assert!(metadata_of(&opf).contains("<meta name=\"cover\" content=\"img-p001-01\"/>"));
        assert!(!spine(&opf).contains("img"));
    }

    #[test]
    fn one_item_is_the_cover_image_and_the_meta_names_that_same_item() {
        let src = format!("{CARD}INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nWork.\n");
        let body = cover_body(
            &src,
            &DEFAULT_FORMAT_OPTIONS,
            &["p001-01.png", "p002-01.png"],
        );
        let epub = build(
            &body,
            &DEFAULT_FORMAT_OPTIONS,
            &[
                image("p002-01.png", png(&[2])),
                image("p001-01.png", png(&[1])),
            ],
        );
        let opf = opf(&epub);
        assert_eq!(image_items(&opf).len(), 2);
        let covers: Vec<String> = image_items(&opf)
            .into_iter()
            .filter(|i| i.contains("properties="))
            .collect();
        assert_eq!(covers.len(), 1);
        assert!(covers[0].contains("href=\"images/p001-01.png\""));
        assert!(covers[0].contains("properties=\"cover-image\""));
        let id = regex::Regex::new(r#"id="([^"]+)""#)
            .unwrap()
            .captures(&covers[0])
            .unwrap()[1]
            .to_owned();
        let metas: Vec<&str> = regex::Regex::new(r#"<meta name="cover"[^>]*/>"#)
            .unwrap()
            .find_iter(&opf)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(metas, [format!("<meta name=\"cover\" content=\"{id}\"/>")]);
    }

    #[test]
    fn a_book_without_a_cover_declares_none() {
        let off = opts(serde_json::json!({ "promoteCoverImage": false }));
        for body in [
            body_showing(&["p001-01.png"], &DEFAULT_FORMAT_OPTIONS),
            cover_body(&format!("{CARD}{SCRIPT}"), &off, &["p001-01.png"]),
        ] {
            assert_eq!(body.cover_image, None);
            let opf = opf(&build(
                &body,
                &DEFAULT_FORMAT_OPTIONS,
                &[image("p001-01.png", png(&[1]))],
            ));
            assert!(!opf.contains("name=\"cover\""));
            assert!(!opf.contains("cover-image"));
        }
    }

    #[test]
    fn a_book_whose_one_picture_is_its_cover_carries_no_figure_rules() {
        let format = opts(serde_json::json!({ "elementSpacingEm": 0.6 }));
        let epub = build(
            &cover_body(&format!("{CARD}{SCRIPT}"), &format, &["p001-01.png"]),
            &format,
            &[image("p001-01.png", png(&[1]))],
        );
        assert_eq!(
            entry_text(&epub, "OEBPS/style.css").unwrap(),
            screenplay_css(&format, false)
        );
    }

    #[test]
    fn a_cover_nobody_handed_over_is_a_caller_bug_and_says_so() {
        let body = card_body();
        assert!(body.used_images.is_empty());
        let err = build_epub(&meta(), &body, &DEFAULT_FORMAT_OPTIONS, &[], EPOCH).unwrap_err();
        assert!(err.to_string().contains("p001-01.png"));
    }
}

mod picture_ids {
    use super::*;

    fn none() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn img_plus_the_name_without_its_extension_reduced_to_letters_digits_underscore_and_dash() {
        assert_eq!(
            image_ids(
                &["p001-01.png", "9-plate.png", "a_b.png", "x.y.z.png"],
                &none()
            ),
            ["img-p001-01", "img-9-plate", "img-a_b", "img-x-y-z"]
        );
    }

    #[test]
    fn names_that_reduce_alike_are_bumped_against_every_id_already_taken() {
        assert_eq!(
            image_ids(&["a.b.png", "a-b.png", "a-b-2.png"], &none()),
            ["img-a-b", "img-a-b-2", "img-a-b-2-2"]
        );
        assert_eq!(
            image_ids(&["a-b-2.png", "a.b.png", "a-b.png"], &none()),
            ["img-a-b-2", "img-a-b", "img-a-b-3"]
        );
    }

    #[test]
    fn the_ids_the_package_document_already_uses_are_taken_too() {
        let taken: HashSet<String> = ["img-cover".to_owned()].into_iter().collect();
        assert_eq!(
            image_ids(&["cover.png", "cover.png"], &taken),
            ["img-cover-2", "img-cover-3"]
        );
    }

    #[test]
    fn every_id_in_a_built_package_document_is_unique_and_a_legal_xml_name() {
        let names = [
            "a.b.png",
            "a-b.png",
            "a-b-2.png",
            "9-plate.png",
            "p001-01.png",
        ];
        let images: Vec<BookImage> = (0u8..)
            .zip(names)
            .map(|(i, n)| image(n, png(&[i])))
            .collect();
        let epub = build(
            &body_showing(&names, &DEFAULT_FORMAT_OPTIONS),
            &DEFAULT_FORMAT_OPTIONS,
            &images,
        );
        let opf = opf(&epub);
        let ids: Vec<String> = regex::Regex::new(r#"\sid="([^"]+)""#)
            .unwrap()
            .captures_iter(&opf)
            .map(|c| c[1].to_owned())
            .collect();
        assert!(ids.contains(&"uid".to_owned()));
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
        for id in &ids {
            assert!(matches(id, r"^[A-Za-z_][A-Za-z0-9_.-]*$"), "{id}");
        }
        assert_eq!(image_items(&opf).len(), 5);
        let mut entries = image_entries(&epub);
        entries.sort();
        let mut want: Vec<String> = names.iter().map(|n| format!("OEBPS/images/{n}")).collect();
        want.sort();
        assert_eq!(entries, want);
    }
}

// ── the reader preview ───────────────────────────────────

/// images.pdf's own .fountain tokens and the pictures its conversion
/// carried, as the oracle dumped them.
fn images_dump() -> (Vec<Token>, Vec<(String, Vec<u8>)>) {
    let dir = repo_root().join("rust/oracle/images");
    let tokens: Vec<Token> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("tokens.json")).unwrap()).unwrap();
    let mut pictures: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir.join("images"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    pictures.sort();
    (tokens, pictures)
}

fn base64_decode(s: &str) -> Vec<u8> {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = u32::try_from(ABC.iter().position(|&x| x == c).expect("base64")).unwrap();
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).unwrap());
        }
    }
    out
}

mod the_preview_carries_the_pictures_it_shows {
    use super::*;

    const DATA: &str = "data:image/png;base64,";

    fn srcs(html: &str) -> Vec<String> {
        regex::Regex::new(r#"<img src="([^"]+)""#)
            .unwrap()
            .captures_iter(html)
            .map(|c| c[1].to_owned())
            .collect()
    }

    #[test]
    fn each_figure_inlines_its_picture_as_a_data_uri_that_decodes_to_the_very_bytes() {
        let (tokens, pictures) = images_dump();
        let no_cover = opts(serde_json::json!({ "promoteCoverImage": false }));
        let html = tokens_to_preview_html(&tokens, &no_cover, &pictures);
        let srcs = srcs(&html);
        assert_eq!(srcs.len(), 2);
        let names: Vec<&str> = pictures.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["p001-01.png", "p002-01.png"]);
        for (i, src) in srcs.iter().enumerate() {
            assert!(src.starts_with(DATA));
            assert_eq!(base64_decode(&src[DATA.len()..]), pictures[i].1);
        }
    }

    #[test]
    fn it_is_the_books_own_body_with_each_src_pointed_at_the_bytes_and_the_sheet_inlined() {
        let (tokens, pictures) = images_dump();
        let format = opts(serde_json::json!({ "elementSpacingEm": 0.6 }));
        let html = tokens_to_preview_html(&tokens, &format, &pictures);
        let mut as_book = html.clone();
        for (name, bytes) in &pictures {
            let src = srcs(&html)
                .into_iter()
                .find(|s| base64_decode(&s[DATA.len()..]) == *bytes);
            if let Some(src) = src {
                as_book = as_book.replace(&src, &format!("../images/{name}"));
            }
        }
        let book = tokens_to_body(
            &tokens,
            &BodyOptions {
                format,
                images: pictures.iter().map(|(n, _)| n.clone()).collect(),
                max_file_bytes: usize::MAX,
                ..BodyOptions::default()
            },
        )
        .files[0]
            .xhtml
            .clone();
        let link = regex::Regex::new(r#"<link rel="stylesheet"[^>]*/>"#).unwrap();
        let want = link.replacen(
            &book,
            1,
            regex::NoExpand(&format!(
                "<style>\n{}</style>",
                screenplay_css(&format, true)
            )),
        );
        assert_eq!(as_book, want);
        assert!(!as_book.contains(DATA));
    }

    #[test]
    fn with_the_title_card_as_the_cover_the_preview_leaves_it_out_as_the_books_pages_do() {
        let (tokens, pictures) = images_dump();
        let html = tokens_to_preview_html(&tokens, &DEFAULT_FORMAT_OPTIONS, &pictures);
        let srcs = srcs(&html);
        assert_eq!(srcs.len(), 1);
        assert_eq!(base64_decode(&srcs[0][DATA.len()..]), pictures[1].1);
        assert!(html.contains(
            "<section class=\"scene\" id=\"sc-001\">\n<h2 class=\"scene-heading\">INT. ARCHIVE ROOM - DAY</h2>"
        ));
    }

    #[test]
    fn with_include_images_off_or_no_pictures_in_hand_the_preview_is_the_one_it_always_was() {
        let (tokens, pictures) = images_dump();
        let on = DEFAULT_FORMAT_OPTIONS;
        let off = opts(serde_json::json!({ "includeImages": false }));
        let plain = tokens_to_preview_html(&tokens, &on, &[]);
        assert_eq!(tokens_to_preview_html(&tokens, &off, &pictures), plain);
        assert_eq!(tokens_to_preview_html(&tokens, &off, &[]), plain);
        assert!(!plain.contains("class=\"figure"));
        assert!(!plain.contains("data:"));
        assert!(plain.contains(&format!("<style>\n{}</style>", screenplay_css(&on, false))));
    }
}

// ── epubcheck ────────────────────────────────────────────

fn epubcheck() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join("epubcheck"))
            .find(|p| p.is_file())
    })
}

/// A scratch folder of this test's own, removed afterwards.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("sp-epub-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn check(tool: &Path, epub: &[u8], tag: &str) -> (bool, String) {
    let scratch = Scratch::new(tag);
    let path = scratch.0.join(format!("{tag}.epub"));
    std::fs::write(&path, epub).unwrap();
    let run = std::process::Command::new(tool)
        .arg(&path)
        .output()
        .unwrap();
    let out = format!(
        "{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    (
        run.status.success() && out.contains("No errors or warnings detected"),
        out,
    )
}

/// The book the pipeline builds from a committed fixture's dump, and its
/// pictures.
fn fixture_book(stem: &str) -> (Vec<u8>, Vec<BookImage>, String) {
    let dir = repo_root().join("rust/oracle").join(stem);
    let tokens: Vec<Token> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("tokens.json")).unwrap()).unwrap();
    let answer: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("convert.json")).unwrap()).unwrap();
    let fountain = std::fs::read_to_string(dir.join("script.fountain")).unwrap();
    let mut images: Vec<BookImage> = std::fs::read_dir(dir.join("images"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| {
            image(
                &e.file_name().to_string_lossy(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    images.sort_by(|a, b| a.name.cmp(&b.name));
    let title = answer["title"].as_str().unwrap().to_owned();
    let body = tokens_to_body(
        &tokens,
        &BodyOptions {
            images: images.iter().map(|i| i.name.clone()).collect(),
            ..BodyOptions::default()
        },
    );
    let meta = BookMeta {
        title: title.clone(),
        author: answer
            .get("author")
            .and_then(|a| a.as_str())
            .map(str::to_owned),
        language: None,
        identifier: Some(content_identifier(&fountain)),
    };
    let epub = build_epub(&meta, &body, &DEFAULT_FORMAT_OPTIONS, &images, EPOCH).unwrap();
    (epub, images, title)
}

mod epubcheck_ {
    use super::*;

    #[test]
    fn the_book_built_from_images_pdf_validates_clean_cover_declared() {
        let Some(tool) = epubcheck() else {
            eprintln!("skipped: epubcheck is not on PATH; brew install epubcheck");
            return;
        };
        let (book, _, _) = fixture_book("images");
        assert_eq!(
            image_entries(&book),
            ["OEBPS/images/p001-01.png", "OEBPS/images/p002-01.png"]
        );
        let opf = opf(&book);
        assert!(opf.contains("<meta name=\"cover\" content=\"img-p001-01\"/>"));
        assert!(opf.contains(
            "href=\"images/p001-01.png\" media-type=\"image/png\" properties=\"cover-image\""
        ));
        let (ok, out) = check(&tool, &book, "images");
        assert!(ok, "{out}");
    }

    #[test]
    fn the_book_built_from_torture_pdf_validates_clean_cover_declared() {
        let Some(tool) = epubcheck() else {
            eprintln!("skipped: epubcheck is not on PATH; brew install epubcheck");
            return;
        };
        let (book, _, _) = fixture_book("torture");
        let opf = opf(&book);
        assert!(opf.contains("<meta name=\"cover\" content=\"img-p001-01\"/>"));
        assert_eq!(opf.matches("properties=\"cover-image\"").count(), 1);
        let (ok, out) = check(&tool, &book, "torture");
        assert!(ok, "{out}");
    }

    #[test]
    fn names_that_reduce_to_the_same_id_still_validate_clean() {
        let Some(tool) = epubcheck() else {
            eprintln!("skipped: epubcheck is not on PATH; brew install epubcheck");
            return;
        };
        let (_, pictures, title) = fixture_book("images");
        let renamed = [
            image("a.b.png", pictures[0].bytes.clone()),
            image("a-b.png", pictures[1].bytes.clone()),
        ];
        let text = "INT. LAB - DAY\n\nBefore.\n\n[[img: a.b.png w=40]]\n\nBetween.\n\n[[img: a-b.png w=100 full]]\n\nAfter.\n";
        let body = tokens_to_body(
            &tokens(text),
            &BodyOptions {
                images: renamed.iter().map(|i| i.name.clone()).collect(),
                ..BodyOptions::default()
            },
        );
        assert_eq!(body.used_images.len(), 2);
        let epub = build_epub(
            &BookMeta { title, ..meta() },
            &body,
            &DEFAULT_FORMAT_OPTIONS,
            &renamed,
            EPOCH,
        )
        .unwrap();
        let (ok, out) = check(&tool, &epub, "collision");
        assert!(ok, "{out}");
    }
}
