//! Port of src/epub/build.ts: EPUB3 packaging. An OCF zip with the mimetype
//! first (stored), package.opf, nav.xhtml (the TOC from scene sections), a
//! generated title page, and the pictures the body shows (stored, #19).

use std::collections::HashSet;
use std::fmt;

use sha2::{Digest, Sha256};
use sp_model::FormatOptions;

use crate::css::screenplay_css;
use crate::html::{BookBody, escape_xml};
use crate::zip::{Method, ZipWriter, utc_fields};

/// `BookMeta`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookMeta {
    pub title: String,
    pub author: Option<String>,
    pub language: Option<String>,
    /// A stable identifier; a random `urn:uuid` when absent.
    pub identifier: Option<String>,
}

/// One picture on its way into a book: the bytes and the name the
/// `.fountain` note and the body's `<img>` both call it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookImage {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// What [`build_epub`] refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpubError {
    /// The body shows, or uses as its cover, pictures it was not given.
    MissingPictures(Vec<String>),
}

impl fmt::Display for EpubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EpubError::MissingPictures(names) => write!(
                f,
                "The book shows or uses as its cover {} but was not given the picture, \
                 so it would point at a file it does not carry.",
                names.join(", ")
            ),
        }
    }
}

impl std::error::Error for EpubError {}

/// The book's identity is its CONTENT (src/convert.ts): the `.fountain` text
/// is the cache boundary, so a rebuild after an options change is the same
/// book to a reader's library, not a new one to re-index.
pub fn content_identifier(fountain_text: &str) -> String {
    let digest = Sha256::digest(fountain_text.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("urn:screepub:{}", &hex[..32])
}

/// Manifest ids for picture filenames, in order: `p001-01.png` becomes
/// `img-p001-01`. The `img-` prefix keeps an id from starting with a digit;
/// names that reduce alike are bumped `-2`, `-3` against every id taken so
/// far (two items sharing an id is RSC-005 plus RSC-008).
pub fn image_ids<S: AsRef<str>>(names: &[S], taken: &HashSet<String>) -> Vec<String> {
    let mut seen = taken.clone();
    names
        .iter()
        .map(|name| {
            let name = name.as_ref();
            let stem = name.strip_suffix(".png").unwrap_or(name);
            // Per UTF-16 code unit, as the regex replaces: a character
            // outside the BMP becomes two dashes.
            let reduced: String = stem
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        c.to_string()
                    } else {
                        "-".repeat(c.len_utf16())
                    }
                })
                .collect();
            let base = format!("img-{reduced}");
            let mut id = base.clone();
            let mut n = 2;
            while seen.contains(&id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            seen.insert(id.clone());
            id
        })
        .collect()
}

/// The pictures to put in the container: each name the book points at, once,
/// in the order it first points at it, with the first bytes passed under
/// that name. A name with no bytes behind it is an error: the `<img>` is
/// already in the body.
fn pictures_to_ship<'a>(
    used: &[String],
    images: &'a [BookImage],
) -> Result<Vec<&'a BookImage>, EpubError> {
    let mut shipped = Vec::new();
    let mut missing = Vec::new();
    for name in used {
        match images.iter().find(|img| &img.name == name) {
            Some(img) => shipped.push(img),
            None => missing.push(name.clone()),
        }
    }
    if missing.is_empty() {
        Ok(shipped)
    } else {
        Err(EpubError::MissingPictures(missing))
    }
}

fn container_xml() -> &'static str {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">
  <rootfiles>
    <rootfile full-path=\"OEBPS/package.opf\" media-type=\"application/oebps-package+xml\"/>
  </rootfiles>
</container>
"
}

fn package_opf(
    meta: &BookMeta,
    identifier: &str,
    body: &BookBody,
    modified: &str,
    include_title_page: bool,
    images: &[&BookImage],
) -> String {
    let manifest: Vec<String> = body
        .files
        .iter()
        .map(|f| {
            format!(
                "    <item id=\"{id}\" href=\"text/{id}.xhtml\" media-type=\"application/xhtml+xml\"/>",
                id = f.id
            )
        })
        .collect();
    // Picture ids are assigned against every id this document writes.
    let mut taken: HashSet<String> = ["uid", "nav", "css", "titlepage"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    taken.extend(body.files.iter().map(|f| f.id.clone()));
    let names: Vec<&str> = images.iter().map(|img| img.name.as_str()).collect();
    let ids = image_ids(&names, &taken);
    // The cover, found in the list the CONTAINER is getting, so a
    // declaration can only ever name an item this document writes.
    let cover_at = body
        .cover_image
        .as_ref()
        .and_then(|c| images.iter().position(|img| &img.name == c));
    let image_items: String = images
        .iter()
        .enumerate()
        .map(|(i, img)| {
            format!(
                "\n    <item id=\"{}\" href=\"images/{}\" media-type=\"image/png\"{}/>",
                ids[i],
                escape_xml(&img.name),
                if Some(i) == cover_at {
                    " properties=\"cover-image\""
                } else {
                    ""
                }
            )
        })
        .collect();
    // BOTH cover spellings: EPUB 3's `cover-image` and OPF 2's meta.
    let cover_meta = cover_at.map_or_else(String::new, |i| {
        format!("\n    <meta name=\"cover\" content=\"{}\"/>", ids[i])
    });
    let spine: Vec<String> = body
        .files
        .iter()
        .map(|f| format!("    <itemref idref=\"{}\"/>", f.id))
        .collect();
    let creator = match meta.author.as_deref() {
        Some(a) if !a.is_empty() => format!("    <dc:creator>{}</dc:creator>\n", escape_xml(a)),
        _ => String::new(),
    };
    let title_item = if include_title_page {
        "    <item id=\"titlepage\" href=\"titlepage.xhtml\" media-type=\"application/xhtml+xml\"/>\n"
    } else {
        ""
    };
    let title_ref = if include_title_page {
        "    <itemref idref=\"titlepage\"/>\n"
    } else {
        ""
    };
    // ibooks:specified-fonts keeps Apple Books honoring the publisher font
    // and text-align; the prefix keeps epubcheck quiet about it.
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"en\" prefix=\"ibooks: http://vocabulary.itunes.apple.com/rdf/ibooks/vocabulary-extensions-1.0/\">
  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">
    <dc:identifier id=\"uid\">{identifier}</dc:identifier>
    <dc:title>{title}</dc:title>
{creator}    <dc:language>{language}</dc:language>
    <meta property=\"dcterms:modified\">{modified}</meta>
    <meta property=\"ibooks:specified-fonts\">true</meta>{cover_meta}
  </metadata>
  <manifest>
    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>
    <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>
{title_item}{manifest}{image_items}
  </manifest>
  <spine>
{title_ref}{spine}
  </spine>
</package>
",
        identifier = escape_xml(identifier),
        title = escape_xml(&meta.title),
        language = meta.language.as_deref().unwrap_or("en"),
        manifest = manifest.join("\n"),
        spine = spine.join("\n"),
    )
}

fn nav_xhtml(meta: &BookMeta, body: &BookBody, include_title_page: bool) -> String {
    let items: Vec<String> = body
        .toc
        .iter()
        .map(|e| {
            format!(
                "      <li><a href=\"{}\">{}</a></li>",
                e.href,
                escape_xml(&e.title)
            )
        })
        .collect();
    // The script's ORIGINAL printed pagination, when the book carries
    // markers.
    let page_list = if body.page_list.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = body
            .page_list
            .iter()
            .map(|p| {
                format!(
                    "    <li><a href=\"{}\">{}</a></li>",
                    p.href,
                    escape_xml(&p.label)
                )
            })
            .collect();
        format!(
            "<nav epub:type=\"page-list\" hidden=\"hidden\">\n  <ol>\n{}\n  </ol>\n</nav>\n",
            entries.join("\n")
        )
    };
    let title_landmark = if include_title_page {
        "    <li><a epub:type=\"titlepage\" href=\"titlepage.xhtml\">Title Page</a></li>\n"
    } else {
        ""
    };
    let first = body.files.first().map_or("body001", |f| f.id.as_str());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE html>
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">
<head>
<title>{title}</title>
<link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>
</head>
<body>
<nav epub:type=\"toc\" id=\"toc\">
  <h1>Scenes</h1>
  <ol>
{items}
  </ol>
</nav>
<nav epub:type=\"landmarks\" hidden=\"hidden\">
  <ol>
{title_landmark}    <li><a epub:type=\"bodymatter\" href=\"text/{first}.xhtml\">Begin Reading</a></li>
  </ol>
</nav>
{page_list}</body>
</html>
",
        title = escape_xml(&meta.title),
        items = items.join("\n"),
    )
}

fn title_page_xhtml(meta: &BookMeta) -> String {
    let author = match meta.author.as_deref() {
        Some(a) if !a.is_empty() => format!(
            "<p class=\"credit\">Written by</p>\n<p class=\"author\">{}</p>\n",
            escape_xml(a)
        ),
        _ => String::new(),
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE html>
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">
<head>
<title>{title}</title>
<link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>
</head>
<body>
<section class=\"titlepage\" epub:type=\"titlepage\">
<h1>{title}</h1>
{author}</section>
</body>
</html>
",
        title = escape_xml(&meta.title),
    )
}

/// `new Date(ms).toISOString()` with the milliseconds dropped.
pub fn iso_seconds(epoch_seconds: i64) -> String {
    let (y, mo, d, h, mi, s) = utc_fields(epoch_seconds);
    let year = if (0..=9999).contains(&y) {
        format!("{y:04}")
    } else if y < 0 {
        format!("-{:06}", -y)
    } else {
        format!("+{y:06}")
    };
    format!("{year}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// The book's clock: `SOURCE_DATE_EPOCH` (seconds, as the README's contract
/// says) when it is set to a number, else now. Used for the package
/// document's `dcterms:modified` and every zip entry's date.
pub fn source_date_epoch() -> i64 {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|raw| raw.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite())
        .map_or_else(now_seconds, |s| {
            // A Date holds whole milliseconds; the book shows whole seconds.
            #[allow(clippy::cast_possible_truncation)]
            let ms = (s * 1000.0).trunc() as i64;
            ms.div_euclid(1000)
        })
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// A random `urn:uuid` (version 4), for a book with no identifier given.
fn random_urn_uuid() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        // RandomState is seeded from the OS per instance.
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_usize(i);
        h.write_i64(now_seconds());
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// `buildEpub(meta, body, format, images)`: a complete EPUB3 file.
///
/// `images` is every picture the caller HAS; the container carries exactly
/// what the body shows plus its cover, and a name the body points at with
/// no bytes behind it is an error. `epoch_seconds` is the book's clock
/// ([`source_date_epoch`] for the CLI's).
pub fn build_epub(
    meta: &BookMeta,
    body: &BookBody,
    format: &FormatOptions,
    images: &[BookImage],
    epoch_seconds: i64,
) -> Result<Vec<u8>, EpubError> {
    // The cover first: it stands ahead of every figure.
    let mut pointed_at: Vec<String> = Vec::new();
    for name in body.cover_image.iter().chain(&body.used_images) {
        if !pointed_at.contains(name) {
            pointed_at.push(name.clone());
        }
    }
    let shipped = pictures_to_ship(&pointed_at, images)?;
    let identifier = meta.identifier.clone().unwrap_or_else(random_urn_uuid);
    let modified = iso_seconds(epoch_seconds);

    let mut zip = ZipWriter::new(epoch_seconds);
    // OCF: mimetype must be the first entry and stored uncompressed.
    zip.file("mimetype", b"application/epub+zip", Method::Stored);
    zip.file(
        "META-INF/container.xml",
        container_xml().as_bytes(),
        Method::Deflated,
    );
    zip.file(
        "OEBPS/package.opf",
        package_opf(
            meta,
            &identifier,
            body,
            &modified,
            format.include_title_page,
            &shipped,
        )
        .as_bytes(),
        Method::Deflated,
    );
    zip.file(
        "OEBPS/nav.xhtml",
        nav_xhtml(meta, body, format.include_title_page).as_bytes(),
        Method::Deflated,
    );
    // The figure rules ride only in a book whose pages show a figure.
    zip.file(
        "OEBPS/style.css",
        screenplay_css(format, !body.used_images.is_empty()).as_bytes(),
        Method::Deflated,
    );
    if format.include_title_page {
        zip.file(
            "OEBPS/titlepage.xhtml",
            title_page_xhtml(meta).as_bytes(),
            Method::Deflated,
        );
    }
    for f in &body.files {
        zip.file(
            &format!("OEBPS/text/{}.xhtml", f.id),
            f.xhtml.as_bytes(),
            Method::Deflated,
        );
    }
    // The same list, in the same order, as the manifest items. STORED: a
    // PNG is already deflated.
    for img in &shipped {
        zip.file(
            &format!("OEBPS/images/{}", img.name),
            &img.bytes,
            Method::Stored,
        );
    }
    Ok(zip.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(iso_seconds(1_767_225_600), "2026-01-01T00:00:00Z");
        assert_eq!(iso_seconds(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn identifiers_hash_the_fountain_text() {
        let id = content_identifier("INT. A - DAY\n");
        assert!(id.starts_with("urn:screepub:"));
        assert_eq!(id.len(), "urn:screepub:".len() + 32);
    }

    #[test]
    fn random_ids_are_v4_uuids() {
        let a = random_urn_uuid();
        assert_eq!(a.len(), "urn:uuid:".len() + 36);
        assert_eq!(&a[23..24], "4");
        assert_ne!(a, random_urn_uuid());
    }
}
