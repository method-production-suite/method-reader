//! The EPUB, read back out of its zip for the AZW3 writer: the same bytes
//! every other route sends, so the Kindle file can never disagree with the
//! book it was made from.
//!
//! Written for the EPUBs sp-epub makes (EPUB 3, one package, a nav document,
//! PNG pictures), and strict about them: a missing package, spine item or
//! picture fails rather than leaving a hole in the Kindle file.
//!
//! Ports src/azw3/epub.ts.

use std::io::{Cursor, Read};

use sp_fountain::js;

use crate::Azw3Error;
use crate::xml::{XDocument, XElement, local_name, parse_xml};

/// One table-of-contents entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubTocEntry {
    pub title: String,
    /// Package-relative path, with any `#fragment`.
    pub href: String,
    /// 0 at the top level.
    pub depth: usize,
    pub children: Vec<EpubTocEntry>,
}

/// One nav landmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubLandmark {
    /// The nav's `epub:type`: `bodymatter`, `titlepage`, `toc`...
    pub kind: String,
    pub title: String,
    pub href: String,
}

/// One raster picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubImage {
    pub href: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

/// One content document of the spine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineDoc {
    pub href: String,
    pub doc: XDocument,
}

/// What the AZW3 writer reads out of an EPUB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubBook {
    pub title: String,
    pub authors: Vec<String>,
    pub language: String,
    pub identifier: Option<String>,
    /// dcterms:modified, when present.
    pub modified: Option<String>,
    /// Content documents in reading order.
    pub spine: Vec<SpineDoc>,
    /// Stylesheets in manifest order: href and text.
    pub stylesheets: Vec<(String, String)>,
    /// Raster pictures in manifest order.
    pub images: Vec<EpubImage>,
    /// The cover picture's href, when the package names one.
    pub cover_href: Option<String>,
    pub toc: Vec<EpubTocEntry>,
    pub landmarks: Vec<EpubLandmark>,
}

const RASTER: [&str; 3] = ["image/png", "image/jpeg", "image/gif"];

/// `decodeURIComponent`: percent-escapes decoded as UTF-8, a malformed one
/// refused.
fn decode_uri_component(s: &str) -> Result<String, Azw3Error> {
    if !s.contains('%') {
        return Ok(s.to_owned());
    }
    let malformed = || {
        Azw3Error::new(format!(
            "The EPUB names {s}, which is not a well-formed URI."
        ))
    };
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = b.get(i + 1..i + 3).ok_or_else(malformed)?;
            let hex = std::str::from_utf8(hex).map_err(|_| malformed())?;
            if !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(malformed());
            }
            out.push(u8::from_str_radix(hex, 16).map_err(|_| malformed())?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}

/// Split off a `#fragment`.
pub fn split_fragment(href: &str) -> (&str, Option<&str>) {
    match href.find('#') {
        Some(h) => (&href[..h], Some(&href[h + 1..])),
        None => (href, None),
    }
}

/// Resolve `rel` against the folder `base` (both zip paths), with `..` and
/// `.` folded, percent-escapes decoded and any fragment kept.
pub fn resolve_href(base: &str, rel: &str) -> Result<String, Azw3Error> {
    let (path, fragment) = split_fragment(rel);
    let mut parts: Vec<String> = if path.starts_with('/') {
        Vec::new()
    } else {
        base.split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    };
    for seg in decode_uri_component(path)?.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s.to_owned()),
        }
    }
    let joined = parts.join("/");
    Ok(match fragment {
        Some(f) => format!("{joined}#{f}"),
        None => joined,
    })
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |s| &path[..s])
}

/// Package-relative path for a zip path, given the package's folder.
fn relative_to(opf_dir: &str, zip_path: &str) -> String {
    if opf_dir.is_empty() {
        return zip_path.to_owned();
    }
    if zip_path == opf_dir {
        return String::new();
    }
    zip_path
        .strip_prefix(opf_dir)
        .and_then(|r| r.strip_prefix('/'))
        .unwrap_or(zip_path)
        .to_owned()
}

struct Zip {
    archive: zip::ZipArchive<Cursor<Vec<u8>>>,
}

impl Zip {
    fn bytes(&mut self, path: &str) -> Option<Vec<u8>> {
        let mut f = self.archive.by_name(path).ok()?;
        let mut out = Vec::new();
        f.read_to_end(&mut out).ok()?;
        Some(out)
    }

    fn text(&mut self, path: &str) -> Result<String, Azw3Error> {
        let b = self
            .bytes(path)
            .ok_or_else(|| Azw3Error::new(format!("The EPUB has no {path}.")))?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }
}

fn nav_list(ol: &XElement, base: &str, depth: usize) -> Result<Vec<EpubTocEntry>, Azw3Error> {
    let mut out = Vec::new();
    for li in ol.child_elements().filter(|e| local_name(&e.name) == "li") {
        let a = li.child_elements().find(|e| local_name(&e.name) == "a");
        let sub = li.child_elements().find(|e| local_name(&e.name) == "ol");
        let children = match sub {
            Some(sub) => nav_list(sub, base, depth + 1)?,
            None => Vec::new(),
        };
        match a.and_then(|a| a.get_attr("href").filter(|h| !h.is_empty()).map(|h| (a, h))) {
            Some((a, href)) => out.push(EpubTocEntry {
                title: js::trim(&a.text_content()).to_owned(),
                href: resolve_href(base, href)?,
                depth,
                children,
            }),
            // A heading with no link of its own: its children stand in its
            // place.
            None => out.extend(children),
        }
    }
    Ok(out)
}

/// The value of an element's `prefix:type` attribute.
fn epub_type(el: &XElement) -> Option<&str> {
    el.attrs
        .iter()
        .find(|(k, _)| local_name(k) == "type" && k.contains(':'))
        .map(|(_, v)| v.as_str())
}

struct Item {
    id: String,
    zip_path: String,
    media_type: String,
    properties: Vec<String>,
}

/// Read an EPUB's bytes.
pub fn read_epub(bytes: &[u8]) -> Result<EpubBook, Azw3Error> {
    let archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec()))
        .map_err(|e| Azw3Error::new(format!("The EPUB is not a zip: {e}")))?;
    let mut zip = Zip { archive };
    let container = parse_xml(&zip.text("META-INF/container.xml")?)?;
    let opf_path = container
        .root
        .find_all("rootfile")
        .first()
        .and_then(|r| r.get_attr("full-path"))
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Azw3Error::new("The EPUB names no package document."))?;
    let opf_dir = dir_of(&opf_path).to_owned();
    let opf = parse_xml(&zip.text(&opf_path)?)?.root;

    let meta = opf.find_all("metadata").into_iter().next();
    let dc = |name: &str| -> Vec<String> {
        meta.map(|m| {
            m.find_all(name)
                .into_iter()
                .map(|e| js::trim(&e.text_content()).to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
    };
    let metas: Vec<&XElement> = meta.map(|m| m.find_all("meta")).unwrap_or_default();

    // Manifest: zip paths resolved from the package's folder.
    let mut items = Vec::new();
    for e in opf.find_all("item") {
        items.push(Item {
            id: e.get_attr("id").unwrap_or("").to_owned(),
            zip_path: resolve_href(&opf_dir, e.get_attr("href").unwrap_or(""))?,
            media_type: e.get_attr("media-type").unwrap_or("").to_owned(),
            properties: e
                .get_attr("properties")
                .unwrap_or("")
                .split(js::is_space)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        });
    }
    // id → item, the last of a repeated id winning, as a Map built from the
    // list does.
    let by_id = |id: &str| items.iter().rev().find(|i| i.id == id);

    let mut spine = Vec::new();
    for r in opf.find_all("itemref") {
        let idref = r.get_attr("idref").unwrap_or("");
        let item = by_id(idref).ok_or_else(|| {
            Azw3Error::new(format!(
                "The spine names {}, which the manifest does not hold.",
                r.get_attr("idref").unwrap_or("undefined")
            ))
        })?;
        let doc = parse_xml(&zip.text(&item.zip_path)?)?;
        spine.push(SpineDoc {
            href: relative_to(&opf_dir, &item.zip_path),
            doc,
        });
    }

    let mut stylesheets: Vec<(String, String)> = Vec::new();
    let mut images = Vec::new();
    for item in &items {
        if item.media_type == "text/css" {
            let href = relative_to(&opf_dir, &item.zip_path);
            let text = zip.text(&item.zip_path)?;
            match stylesheets.iter_mut().find(|(h, _)| *h == href) {
                Some(found) => found.1 = text,
                None => stylesheets.push((href, text)),
            }
        } else if RASTER.contains(&item.media_type.as_str()) {
            let bytes = zip.bytes(&item.zip_path).ok_or_else(|| {
                Azw3Error::new(format!(
                    "The EPUB lists {} but does not carry it.",
                    item.zip_path
                ))
            })?;
            images.push(EpubImage {
                href: relative_to(&opf_dir, &item.zip_path),
                media_type: item.media_type.clone(),
                bytes,
            });
        }
    }

    // The cover: EPUB 3's cover-image property, else OPF 2's meta.
    let cover_meta = metas.iter().find(|m| m.get_attr("name") == Some("cover"));
    let cover_item = items
        .iter()
        .find(|i| i.properties.iter().any(|p| p == "cover-image"))
        .or_else(|| cover_meta.and_then(|m| by_id(m.get_attr("content").unwrap_or(""))));
    let cover_href = cover_item
        .filter(|i| RASTER.contains(&i.media_type.as_str()))
        .map(|i| relative_to(&opf_dir, &i.zip_path));

    // The nav document: the table of contents and the landmarks.
    let mut toc = Vec::new();
    let mut landmarks = Vec::new();
    if let Some(nav_item) = items
        .iter()
        .find(|i| i.properties.iter().any(|p| p == "nav"))
    {
        let nav = parse_xml(&zip.text(&nav_item.zip_path)?)?.root;
        let nav_base = relative_to(&opf_dir, dir_of(&nav_item.zip_path));
        for n in nav.find_all("nav") {
            let Some(ol) = n.child_elements().find(|e| local_name(&e.name) == "ol") else {
                continue;
            };
            let kind = epub_type(n);
            if kind == Some("toc") {
                toc = nav_list(ol, &nav_base, 0)?;
            }
            if kind == Some("landmarks") {
                for a in ol.find_all("a") {
                    let href = a.get_attr("href").filter(|h| !h.is_empty());
                    let t = epub_type(a).filter(|t| !t.is_empty());
                    if let (Some(href), Some(t)) = (href, t) {
                        landmarks.push(EpubLandmark {
                            kind: t.to_owned(),
                            title: js::trim(&a.text_content()).to_owned(),
                            href: resolve_href(&nav_base, href)?,
                        });
                    }
                }
            }
        }
    }

    let modified = metas
        .iter()
        .find(|m| m.get_attr("property") == Some("dcterms:modified"))
        .map(|m| js::trim(&m.text_content()).to_owned());
    Ok(EpubBook {
        title: dc("title")
            .into_iter()
            .next()
            .unwrap_or_else(|| "Unknown".to_owned()),
        authors: dc("creator"),
        language: dc("language")
            .into_iter()
            .next()
            .unwrap_or_else(|| "en".to_owned()),
        identifier: dc("identifier").into_iter().next(),
        modified,
        spine,
        stylesheets,
        images,
        cover_href,
        toc,
        landmarks,
    })
}
