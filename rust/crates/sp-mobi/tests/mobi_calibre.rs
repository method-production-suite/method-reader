//! Calibre's MOBI reader, run on our own files: an outside reader of the
//! container (ports the two Calibre round trips in tests/mobi.test.ts).
//! Only ever through tests/common/calibre.rs's Sandbox, which hands Calibre
//! a scratch copy of its settings folder; skips cleanly where Calibre is
//! not installed.

mod common;

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use common::calibre::{Sandbox, calibre_tool, real_calibre_config};
use common::*;
use regex::Regex;
use sp_mobi::{MobiInput, build_mobi};

/// A real JPEG, small enough to spell out: mid grey, eight pixels tall and
/// eight per block wide (tests/mobi.test.ts's greyJpeg).
fn grey_jpeg(blocks: usize) -> Vec<u8> {
    fn segment(marker: u8, body: &[u8]) -> Vec<u8> {
        let n = body.len() + 2;
        [&[0xff, marker, (n >> 8) as u8, (n & 0xff) as u8][..], body].concat()
    }
    let width = 8 * blocks;
    let mut bits = "00".repeat(blocks);
    while !bits.len().is_multiple_of(8) {
        bits.push('1');
    }
    let scan: Vec<u8> = bits
        .as_bytes()
        .chunks(8)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 2).unwrap())
        .collect();
    let mut one_code = vec![1u8];
    one_code.extend([0u8; 15]);
    one_code.push(0);
    let mut dqt = vec![0u8];
    dqt.extend([1u8; 64]);
    [
        vec![0xff, 0xd8],
        segment(
            0xe0,
            &[0x4a, 0x46, 0x49, 0x46, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0],
        ),
        segment(0xdb, &dqt),
        segment(
            0xc0,
            &[
                8,
                0,
                8,
                (width >> 8) as u8,
                (width & 0xff) as u8,
                1,
                1,
                0x11,
                0,
            ],
        ),
        segment(0xc4, &[[0x00].as_slice(), &one_code].concat()),
        segment(0xc4, &[[0x10].as_slice(), &one_code].concat()),
        segment(0xda, &[1, 1, 0, 0, 63, 0]),
        scan,
        vec![0xff, 0xd9],
    ]
    .concat()
}

/// What Calibre's EPUB of a MOBI holds: its package and every file.
struct Back {
    opf: String,
    files: BTreeMap<String, Vec<u8>>,
    /// manifest id → zip path
    hrefs: BTreeMap<String, String>,
}

fn join_posix(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn dirname(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i])
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Some(v) = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn through_calibre(sandbox: &Sandbox, tool: &Path, mobi: &[u8], name: &str) -> Back {
    let mobi_path = sandbox.dir().join(format!("{name}.mobi"));
    let epub_path = sandbox.dir().join(format!("{name}-from-mobi.epub"));
    std::fs::write(&mobi_path, mobi).unwrap();
    // No generated cover: it would be a picture this book never had.
    let run = sandbox.run(
        tool,
        &[
            mobi_path.to_str().unwrap(),
            epub_path.to_str().unwrap(),
            "--no-default-epub-cover",
        ],
        sandbox.dir(),
    );
    assert_eq!(run.code, 0, "{}", run.out);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&epub_path).unwrap()).unwrap();
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        files.insert(f.name().unwrap().into_owned(), b);
    }
    let text = |p: &str| String::from_utf8(files[p].clone()).unwrap();
    let container = text("META-INF/container.xml");
    let opf_path = Regex::new(r#"full-path="([^"]+)""#)
        .unwrap()
        .captures(&container)
        .unwrap()[1]
        .to_owned();
    let opf = text(&opf_path);
    let mut hrefs = BTreeMap::new();
    let id_re = Regex::new(r#"\bid="([^"]+)""#).unwrap();
    let href_re = Regex::new(r#"\bhref="([^"]+)""#).unwrap();
    for item in Regex::new(r"<item\b[^>]*>").unwrap().find_iter(&opf) {
        if let (Some(id), Some(href)) = (
            id_re.captures(item.as_str()),
            href_re.captures(item.as_str()),
        ) {
            hrefs.insert(
                id[1].to_owned(),
                join_posix(dirname(&opf_path), &percent_decode(&href[1])),
            );
        }
    }
    Back { opf, files, hrefs }
}

impl Back {
    /// Every picture an `<img>` on a spine page shows, in reading order.
    fn shown(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let src = Regex::new(r#"<img\b[^>]*\bsrc="([^"]+)""#).unwrap();
        for idref in Regex::new(r#"<itemref\b[^>]*\bidref="([^"]+)""#)
            .unwrap()
            .captures_iter(&self.opf)
        {
            let doc = &self.hrefs[&idref[1]];
            let xhtml = String::from_utf8(self.files[doc].clone()).unwrap();
            for m in src.captures_iter(&xhtml) {
                out.push(self.files[&join_posix(dirname(doc), &percent_decode(&m[1]))].clone());
            }
        }
        out
    }

    fn cover_id(&self) -> Option<String> {
        Regex::new(r#"<meta\b[^>]*\bname="cover"[^>]*\bcontent="([^"]+)""#)
            .unwrap()
            .captures(&self.opf)
            .or_else(|| {
                Regex::new(r#"<meta\b[^>]*\bcontent="([^"]+)"[^>]*\bname="cover""#)
                    .unwrap()
                    .captures(&self.opf)
            })
            .map(|c| c[1].to_owned())
    }

    /// The picture the package names as its cover.
    fn cover(&self) -> Option<Vec<u8>> {
        let href = self.hrefs.get(&self.cover_id()?)?;
        self.files.get(href).cloned()
    }
}

/// Every file under the real Calibre folder, with its size and time, so a
/// test can prove the real folder was not touched.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (u64, Option<SystemTime>)> {
    let mut out = BTreeMap::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in read.flatten() {
        let p = e.path();
        if let Ok(m) = std::fs::symlink_metadata(&p) {
            if m.is_dir() {
                out.extend(snapshot(&p));
            } else {
                out.insert(p, (m.len(), m.modified().ok()));
            }
        }
    }
    out
}

fn skip(why: &str) {
    eprintln!("skipped: {why}");
}

#[test]
fn calibre_takes_a_jpeg_cover_from_the_record_exth_201_names() {
    let Some(tool) = calibre_tool("ebook-convert") else {
        return skip("Calibre's ebook-convert was not found");
    };
    let real = real_calibre_config();
    let before = snapshot(&real);
    let sandbox = Sandbox::new("mobi-calibre-cover");

    let pictures = vec![grey_jpeg(1), grey_jpeg(2)];
    let html = "<html><head></head><body><p>Before.</p>\n<p align=\"center\"><img recindex=\"00001\" alt=\"Illustration\"/></p>\n<p align=\"center\"><img recindex=\"00002\" alt=\"Illustration\"/></p>\n<p>After.</p></body></html>";
    let make = |cover_index| {
        build_mobi(&MobiInput {
            title: "Cover Probe".into(),
            html: html.into(),
            images: pictures.clone(),
            cover_index,
            time: Some(1_767_225_600),
            ..Default::default()
        })
        .unwrap()
    };
    let named = through_calibre(&sandbox, &tool, &make(Some(1)), "jpeg-cover");
    assert_eq!(named.cover(), Some(pictures[1].clone()));
    assert_eq!(named.shown(), pictures);
    let unnamed = through_calibre(&sandbox, &tool, &make(None), "jpeg-no-cover");
    assert_eq!(unnamed.cover(), Some(pictures[0].clone()));
    assert_eq!(unnamed.shown(), pictures);

    drop(sandbox);
    assert_eq!(
        snapshot(&real),
        before,
        "the real Calibre folder must not change"
    );
}

#[test]
fn calibre_reads_every_picture_of_images_back_in_order() {
    let Some(tool) = calibre_tool("ebook-convert") else {
        return skip("Calibre's ebook-convert was not found");
    };
    let real = real_calibre_config();
    let before = snapshot(&real);
    let sandbox = Sandbox::new("mobi-calibre-images");

    let dir = workspace().join("oracle/images");
    let tokens: Vec<sp_model::Token> =
        serde_json::from_slice(&std::fs::read(dir.join("tokens.json")).unwrap()).unwrap();
    let mut pictures = BTreeMap::new();
    for name in ["p001-01.png", "p002-01.png"] {
        pictures.insert(
            name.to_owned(),
            std::fs::read(dir.join("images").join(name)).unwrap(),
        );
    }
    let have: HashSet<String> = pictures.keys().cloned().collect();
    let format = sp_model::DEFAULT_FORMAT_OPTIONS;
    let art = sp_mobi::mobi_art(&tokens, &format, &have);
    let meta = sp_mobi::MobiMeta {
        title: "The Paper Lantern".into(),
        author: Some("J. Q. Nobody".into()),
    };
    let html = sp_mobi::tokens_to_mobi_html(&tokens, &meta, &format, &art);
    let mobi = build_mobi(&MobiInput {
        title: meta.title.clone(),
        author: meta.author.clone(),
        html,
        images: art.order.iter().map(|f| pictures[f].clone()).collect(),
        cover_index: art.cover_index,
        time: Some(1_767_225_600),
    })
    .unwrap();
    // The card is a PNG, and calibre looks a cover up only under a .jpg
    // name, so it takes no cover from this book: which is why the card
    // stays on its page, first in the flow.
    assert_eq!(art.cover_index, Some(0));
    assert_eq!(exth(&mobi, 201), Some(vec![0, 0, 0, 0]));
    let back = through_calibre(&sandbox, &tool, &mobi, "images");
    let fountain = std::fs::read_to_string(dir.join("script.fountain")).unwrap();
    let in_script_order: Vec<Vec<u8>> = fountain
        .lines()
        .filter_map(|l| l.strip_prefix("[[img: "))
        .map(|rest| pictures[rest.split_whitespace().next().unwrap()].clone())
        .collect();
    assert_eq!(back.shown(), in_script_order);
    assert!(back.cover_id().is_none());
    assert!(back.cover().is_none());

    drop(sandbox);
    assert_eq!(
        snapshot(&real),
        before,
        "the real Calibre folder must not change"
    );
}
