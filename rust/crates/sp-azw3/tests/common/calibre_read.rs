//! Calibre's own reader, run on an AZW3 (ports tests/helpers/calibre-read.ts).
//! ONLY through calibre.rs's Sandbox, which hands Calibre a scratch copy of
//! its settings folder. `calibre-debug --inspect-mobi` writes its decompiled
//! folder into the working directory, so the cwd is the book's own scratch
//! folder.

use std::io::{Cursor, Read};
use std::path::Path;

use sp_azw3::epub::read_epub;
use sp_azw3::xml::{XElement, local_name, parse_xml};
use sp_fountain::js;

use super::calibre::{Run, Sandbox};

/// Whitespace and soft hyphens folded, and the non-breaking hyphen (U+2011)
/// read as a plain one: Calibre's conversion back to EPUB writes it as "-".
pub fn norm(s: &str) -> String {
    let s = s.replace('\u{2011}', "-");
    s.split(|c: char| js::is_space(c) || c == '\u{ad}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a reader of an EPUB sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookView {
    pub title: String,
    pub authors: Vec<String>,
    pub language: String,
    pub toc: Vec<String>,
    /// The text of each p, h1 and h2, in reading order.
    pub blocks: Vec<String>,
    /// All the body text, whitespace folded.
    pub text: String,
    pub images: Vec<Vec<u8>>,
    pub cover: Option<Vec<u8>>,
}

fn blocks_of(body: &XElement, out: &mut Vec<String>) {
    for c in body.child_elements() {
        let name = local_name(&c.name);
        if name == "p" || name == "h1" || name == "h2" {
            let t = norm(&c.text_content());
            if !t.is_empty() {
                out.push(t);
            }
        } else {
            blocks_of(c, out);
        }
    }
}

/// Our own EPUB, as a BookView.
pub fn view_of_our_epub(bytes: &[u8]) -> BookView {
    let book = read_epub(bytes).expect("our EPUB reads");
    let bodies: Vec<&XElement> = book
        .spine
        .iter()
        .map(|s| s.doc.root.find_all("body")[0])
        .collect();
    let mut blocks = Vec::new();
    for b in &bodies {
        blocks_of(b, &mut blocks);
    }
    BookView {
        title: book.title.clone(),
        authors: book.authors.clone(),
        language: book.language.clone(),
        toc: book.toc.iter().map(|t| norm(&t.title)).collect(),
        blocks,
        text: norm(
            &bodies
                .iter()
                .map(|b| b.text_content())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        images: book.images.iter().map(|i| i.bytes.clone()).collect(),
        cover: book
            .images
            .iter()
            .find(|i| Some(&i.href) == book.cover_href.as_ref())
            .map(|i| i.bytes.clone()),
    }
}

fn file(zip: &mut zip::ZipArchive<Cursor<Vec<u8>>>, path: &str) -> Vec<u8> {
    let mut f = zip
        .by_name(path)
        .unwrap_or_else(|_| panic!("the EPUB holds {path}"));
    let mut out = Vec::new();
    f.read_to_end(&mut out).expect("readable");
    out
}

fn text(zip: &mut zip::ZipArchive<Cursor<Vec<u8>>>, path: &str) -> String {
    String::from_utf8_lossy(&file(zip, path)).into_owned()
}

/// An EPUB Calibre (or KindleUnpack) wrote (OPF 2, an NCX), as a BookView.
/// Calibre's generated cover page, marked calibre:cover, is left out of the
/// text: it is Calibre's, not the book's.
pub fn view_of_calibre_epub(bytes: &[u8]) -> BookView {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip");
    let container = parse_xml(&text(&mut zip, "META-INF/container.xml"))
        .unwrap()
        .root;
    let opf_path = container.find_all("rootfile")[0]
        .get_attr("full-path")
        .unwrap()
        .to_owned();
    let opf_dir = opf_path
        .rfind('/')
        .map_or(String::new(), |i| opf_path[..=i].to_owned());
    let opf = parse_xml(&text(&mut zip, &opf_path)).unwrap().root;
    let items: Vec<&XElement> = opf.find_all("item");
    let item = |id: &str| items.iter().find(|i| i.get_attr("id") == Some(id)).copied();
    let meta = opf.find_all("metadata")[0];
    let dc = |n: &str| {
        meta.find_all(n)
            .iter()
            .map(|e| js::trim(&e.text_content()).to_owned())
            .collect::<Vec<_>>()
    };

    let mut bodies = Vec::new();
    for r in opf.find_all("itemref") {
        let it = item(r.get_attr("idref").unwrap()).unwrap();
        let doc = parse_xml(&text(
            &mut zip,
            &format!("{opf_dir}{}", it.get_attr("href").unwrap()),
        ))
        .unwrap()
        .root;
        if doc
            .find_all("meta")
            .iter()
            .any(|m| m.get_attr("name") == Some("calibre:cover"))
        {
            continue;
        }
        bodies.push(doc.find_all("body")[0].clone());
    }
    let toc = match items
        .iter()
        .find(|i| i.get_attr("media-type") == Some("application/x-dtbncx+xml"))
    {
        Some(ncx) => parse_xml(&text(
            &mut zip,
            &format!("{opf_dir}{}", ncx.get_attr("href").unwrap()),
        ))
        .unwrap()
        .root
        .find_all("navLabel")
        .iter()
        .map(|l| norm(&l.text_content()))
        .collect(),
        None => Vec::new(),
    };
    let cover_id = meta
        .find_all("meta")
        .into_iter()
        .find(|m| m.get_attr("name") == Some("cover"))
        .and_then(|m| m.get_attr("content"))
        .map(str::to_owned);
    let images: Vec<Vec<u8>> = items
        .iter()
        .filter(|i| i.get_attr("media-type").unwrap_or("").starts_with("image/"))
        .map(|i| {
            file(
                &mut zip,
                &format!("{opf_dir}{}", i.get_attr("href").unwrap()),
            )
        })
        .collect();
    let cover = cover_id
        .and_then(|id| item(&id).map(|i| i.get_attr("href").unwrap().to_owned()))
        .map(|href| file(&mut zip, &format!("{opf_dir}{href}")));
    let mut blocks = Vec::new();
    for b in &bodies {
        blocks_of(b, &mut blocks);
    }
    BookView {
        title: dc("title").first().cloned().unwrap_or_default(),
        authors: dc("creator"),
        language: dc("language").first().cloned().unwrap_or_default(),
        toc,
        blocks,
        text: norm(
            &bodies
                .iter()
                .map(XElement::text_content)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        images,
        cover,
    }
}

/// Calibre's reader turns `azw3` back into an EPUB beside it, without a
/// default cover (Calibre would otherwise draw one).
pub fn calibre_to_epub(sandbox: &Sandbox, tool: &Path, azw3: &Path) -> (Option<Vec<u8>>, String) {
    let dir = azw3.parent().expect("a folder");
    let out = dir.join(format!(
        "{}.calibre-read.epub",
        azw3.file_stem().unwrap().to_string_lossy()
    ));
    let run = sandbox.run(
        tool,
        &[
            azw3.to_str().unwrap(),
            out.to_str().unwrap(),
            "--no-default-epub-cover",
        ],
        dir,
    );
    let epub = (run.code == 0).then(|| std::fs::read(&out).ok()).flatten();
    (epub, run.out)
}

/// `calibre-debug --inspect-mobi`: the entry counts its index dumps report.
pub fn inspect_mobi(sandbox: &Sandbox, tool: &Path, azw3: &Path) -> (Run, Vec<(String, usize)>) {
    let dir = azw3.parent().expect("a folder");
    let run = sandbox.run(tool, &["--inspect-mobi", azw3.to_str().unwrap()], dir);
    let decompiled = dir.join(format!(
        "decompiled_{}",
        azw3.file_stem().unwrap().to_string_lossy()
    ));
    let mut entries = Vec::new();
    for name in ["skel", "chunks", "ncx", "guide"] {
        let Ok(dump) = std::fs::read_to_string(decompiled.join(format!("{name}.record"))) else {
            continue;
        };
        let key = "Total number of actual Index Entries in all records: ";
        if let Some(at) = dump.find(key) {
            let n: String = dump[at + key.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            entries.push((name.to_owned(), n.parse().unwrap()));
        }
    }
    (run, entries)
}
