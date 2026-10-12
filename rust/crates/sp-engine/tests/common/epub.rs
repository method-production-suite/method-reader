//! Reading a converted book back: the EPUB's zip entries (JSZip's
//! `loadAsync` in the TypeScript tests) and the few views of it the
//! conversion tests share.

use std::io::Read;

/// One zip entry as JSZip lists it: folders included, flagged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub data: Vec<u8>,
}

/// Every entry, in the zip's order (`Object.keys(zip.files)`).
pub fn read_zip(bytes: &[u8]) -> Vec<Entry> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a zip");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).expect("an entry");
            let mut data = Vec::new();
            f.read_to_end(&mut data).expect("entry bytes");
            Entry {
                name: f.name().expect("a name").into_owned(),
                dir: f.is_dir(),
                data,
            }
        })
        .collect()
}

/// One entry's bytes, `None` if the book has no such entry.
pub fn entry(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    read_zip(bytes)
        .into_iter()
        .find(|e| e.name == name)
        .map(|e| e.data)
}

/// One entry's text; panics if it is missing.
pub fn text(bytes: &[u8], name: &str) -> String {
    String::from_utf8(entry(bytes, name).unwrap_or_else(|| panic!("{name} is in the book")))
        .expect("UTF-8")
}

/// `OEBPS/package.opf`.
pub fn opf(bytes: &[u8]) -> String {
    text(bytes, "OEBPS/package.opf")
}

/// The `dc:identifier` the package declares.
pub fn identifier(bytes: &[u8]) -> String {
    let opf = opf(bytes);
    let re = regex::Regex::new(r"<dc:identifier[^>]*>([^<]+)<").unwrap();
    re.captures(&opf).expect("an identifier")[1].to_owned()
}

/// The contents' entries, as (anchor, title) (`tocOf`).
pub fn toc_of(bytes: &[u8]) -> Vec<(String, String)> {
    let nav = text(bytes, "OEBPS/nav.xhtml");
    let from = nav.find("epub:type=\"toc\"").expect("a toc");
    let to = nav.find("</nav>").expect("a nav");
    let toc = &nav[from..to];
    let re = regex::Regex::new(r##"<a href="[^"#]*#(sc-\d+)">([^<]*)</a>"##).unwrap();
    re.captures_iter(toc)
        .map(|c| (c[1].to_owned(), c[2].to_owned()))
        .collect()
}

/// Every body file, in reading order, as one string (`bodyOf`).
pub fn body_of(bytes: &[u8]) -> String {
    let re = regex::Regex::new(r"^OEBPS/text/body\d+\.xhtml$").unwrap();
    let mut files: Vec<Entry> = read_zip(bytes)
        .into_iter()
        .filter(|e| re.is_match(&e.name))
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
        .into_iter()
        .map(|e| String::from_utf8(e.data).expect("UTF-8"))
        .collect()
}

/// The picture files, sorted, folders left out (`imageEntries`).
pub fn image_entries(bytes: &[u8]) -> Vec<String> {
    let mut out: Vec<String> = read_zip(bytes)
        .into_iter()
        .filter(|e| e.name.starts_with("OEBPS/images/") && !e.dir)
        .map(|e| e.name)
        .collect();
    out.sort();
    out
}
