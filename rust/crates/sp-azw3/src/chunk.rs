//! The KF8 markup: our XHTML turned into what a Kindle's KF8 reader
//! expects, and cut into skeletons and fragments.
//!
//! Ported from Calibre 9.11.0's src/calibre/ebooks/mobi/writer8/main.py
//! (the markup passes: resource links, CSS flows, link placeholders, aid
//! attributes) and writer8/skeleton.py (the Chunker). Calibre is GPL-3,
//! Copyright Kovid Goyal; this port is licensed GPL-3.0-only (LICENSE-GPL-3.0)
//! (see THIRD-PARTY-NOTICES.md). Ports src/azw3/chunk.ts.
//!
//! How KF8 stores a document: each XHTML file becomes a SKELETON (the file
//! with most of its body taken out) followed by FRAGMENTS (the pieces taken
//! out, each at most 8192 bytes). The skeleton table says where each file's
//! skeleton sits in the text; the fragment table says, for each piece, the
//! position in the rebuilt file to put it back at. Every element that can
//! be a link target gets an `aid` attribute, and links and the TOC point at
//! a fragment and an offset into it ("pos:fid") rather than at an id.
//!
//! What is left out, because our EPUB never carries it: SVG and `@import`
//! between stylesheets. `<style>` elements are handled, though sp-epub
//! writes none.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use sp_fountain::js;
use sp_fountain::jsre::{Regex, group};

use crate::Azw3Error;
use crate::epub::{EpubBook, resolve_href, split_fragment};
use crate::index::{ChunkEntry, SkelEntry, to_base};
use crate::xml::{XElement, XNode, escape_text, local_name, open_tag, serialize_element};

pub const CHUNK_SIZE: usize = 8192;
const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";

/// Tags kindlegen gives an aid (skeleton.py's aid_able_tags).
pub const AID_ABLE_TAGS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "audio",
    "b",
    "bdo",
    "blockquote",
    "body",
    "button",
    "cite",
    "code",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "i",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "map",
    "mark",
    "meter",
    "nav",
    "ol",
    "output",
    "p",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "samp",
    "section",
    "select",
    "small",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "textarea",
    "time",
    "ul",
    "var",
    "video",
];

fn aid_able(tag: &str) -> bool {
    AID_ABLE_TAGS.contains(&tag)
}

/// Written `<p></p>`, never `<p/>` (skeleton.py's close_self_closing_tags).
static CLOSE_EMPTY: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    AID_ABLE_TAGS
        .iter()
        .copied()
        .chain(["script", "style", "title", "head"])
        .collect()
});

fn ser(el: &XElement) -> String {
    serialize_element(el, Some(&CLOSE_EMPTY))
}

/// A record reference: base 32, four digits (main.py's to_ref).
pub fn to_ref(n: usize) -> String {
    to_base(n as u64, 32, 4)
}

/// A link offset: base 32, ten digits (skeleton.py's to_href).
fn to_href(n: usize) -> String {
    to_base(n as u64, 32, 10)
}

/// A picture's resource number (1-based, as `kindle:embed:XXXX` counts it)
/// and mime type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub index: usize,
    pub mime: String,
}

/// Where an aid lands: fragment number, offset in it, offset in the
/// rebuilt text.
pub type AidOffset = (usize, usize, usize);

/// The KF8 text and tables for a book.
#[derive(Debug, Clone)]
pub struct Kf8Markup {
    /// Every flow, encoded: flow 0 is the text (each file's skeleton then
    /// its fragments), the rest are stylesheets.
    pub flows: Vec<Vec<u8>>,
    pub skel_table: Vec<SkelEntry>,
    pub chunk_table: Vec<ChunkEntry>,
    /// aid → where it lands.
    pub aid_offsets: HashMap<String, AidOffset>,
    /// (href, id) → aid; the empty id is the file's body.
    pub id_map: HashMap<(String, String), String>,
    /// Package hrefs of the pictures the markup shows, in first-shown order.
    pub used_images: Vec<String>,
}

impl Kf8Markup {
    /// The aid for (href, id), falling back to the file's body.
    pub fn aid_for(&self, href: &str, id: &str) -> Option<&String> {
        self.id_map
            .get(&(href.to_owned(), id.to_owned()))
            .or_else(|| self.id_map.get(&(href.to_owned(), String::new())))
    }
}

#[derive(Debug, Clone)]
struct Chunk {
    raw: Vec<u8>,
    starts_tags: Vec<String>,
    ends_tags: Vec<String>,
    insert_pos: usize,
    selector: String,
}

fn dir_of(href: &str) -> &str {
    href.rfind('/').map_or("", |s| &href[..s])
}

/// Every element, preorder, the root included, each handed to `f` before
/// its children are walked (so `f` may change which children there are).
fn walk_mut(
    el: &mut XElement,
    f: &mut impl FnMut(&mut XElement) -> Result<(), Azw3Error>,
) -> Result<(), Azw3Error> {
    f(el)?;
    for c in &mut el.children {
        if let XNode::Element(e) = c {
            walk_mut(e, f)?;
        }
    }
    Ok(())
}

fn text_of(nodes: &[XNode]) -> String {
    nodes
        .iter()
        .map(|n| match n {
            XNode::Text(t) => t.as_str(),
            XNode::Element(_) => "",
        })
        .collect()
}

fn merge_text(el: &mut XElement) {
    let mut out: Vec<XNode> = Vec::with_capacity(el.children.len());
    for c in std::mem::take(&mut el.children) {
        match (out.last_mut(), c) {
            (Some(XNode::Text(last)), XNode::Text(t)) => last.push_str(&t),
            (_, XNode::Text(t)) if t.is_empty() => {}
            (_, c) => out.push(c),
        }
    }
    el.children = out;
    for c in &mut el.children {
        if let XNode::Element(e) = c {
            merge_text(e);
        }
    }
}

/// A copy with every namespace taken off names and attributes, and the
/// namespace declarations gone, so it serializes as plain HTML 5 markup;
/// the XHTML namespace goes back on <html> when the skeleton is written
/// (skeleton.py's remove_namespaces). `epub:type` becomes `type`.
fn remove_namespaces(root: &XElement) -> XElement {
    fn strip(el: &XElement) -> XElement {
        let mut attrs: Vec<(String, String)> = Vec::new();
        for (k, v) in &el.attrs {
            if k == "xmlns" || k.starts_with("xmlns:") {
                continue;
            }
            let name = local_name(k);
            match attrs.iter_mut().find(|(n, _)| n == name) {
                Some(found) => found.1 = v.clone(),
                None => attrs.push((name.to_owned(), v.clone())),
            }
        }
        XElement {
            name: local_name(&el.name).to_owned(),
            attrs,
            children: el
                .children
                .iter()
                .map(|c| match c {
                    XNode::Text(t) => XNode::Text(t.clone()),
                    XNode::Element(e) => XNode::Element(strip(e)),
                })
                .collect(),
        }
    }
    let mut copy = strip(root);
    // The root keeps only its language, class and style.
    let lang = root
        .attrs
        .iter()
        .rfind(|(k, _)| local_name(k) == "lang")
        .map(|(_, v)| v.clone())
        .filter(|l| !l.is_empty());
    let mut attrs = Vec::new();
    if let Some(lang) = lang {
        attrs.push(("lang".to_owned(), lang));
    }
    attrs.extend(
        copy.attrs
            .iter()
            .filter(|(k, _)| k == "class" || k == "style")
            .cloned(),
    );
    copy.attrs = attrs;
    copy.name = "html".to_owned();
    copy
}

struct Chunker {
    selector: String,
    chunks: Vec<Chunk>,
}

impl Chunker {
    fn chunk(&self, raw: Vec<u8>) -> Chunk {
        Chunk {
            raw,
            starts_tags: Vec::new(),
            ends_tags: Vec::new(),
            insert_pos: 0,
            selector: self.selector.clone(),
        }
    }

    fn chunk_up_text(&self, text: &str) -> Vec<Chunk> {
        let escaped = escape_text(text).into_bytes();
        let mut raw: &[u8] = &escaped;
        let take = |raw: &mut &[u8]| -> Vec<u8> {
            if raw.len() <= CHUNK_SIZE {
                let all = raw.to_vec();
                *raw = &[];
                return all;
            }
            let mut end = CHUNK_SIZE;
            while end > 0 && (raw[end] & 0xc0) == 0x80 {
                end -= 1;
            }
            // Nor inside an entity: the text is escaped before it is cut, and
            // an "&amp;" split into "&a" and "mp;" leaves both pieces broken
            // XML. Calibre's chunk_up_text cuts at the byte count alone; this
            // is the one place the port does not follow it. Our escapes are at
            // most five bytes, so an "&" in the last four with no ";" after
            // it is one cut.
            let mut k = end;
            while k > end.saturating_sub(4) {
                k -= 1;
                if raw[k] == b';' {
                    break;
                }
                if raw[k] == b'&' {
                    end = k;
                    break;
                }
            }
            let piece = raw[..end].to_vec();
            *raw = &raw[end..];
            piece
        };
        let mut out = vec![take(&mut raw)];
        while !raw.is_empty() {
            let mut piece = b"<span class=\"AmznBigTextBlock\">".to_vec();
            piece.extend(take(&mut raw));
            piece.extend_from_slice(b"</span>");
            out.push(piece);
        }
        out.into_iter().map(|r| self.chunk(r)).collect()
    }

    /// skeleton.py's step_into_tag: take `tag`'s content out into
    /// fragments, stepping into any child too large for one fragment.
    fn step_into_tag(&mut self, tag: &mut XElement) -> Result<(), Azw3Error> {
        let aid = tag.get_attr("aid").unwrap_or("").to_owned();
        self.selector = format!("P-//*[@aid='{aid}']");
        let first = self.chunks.len();

        // Leading text; pure whitespace stays in the skeleton.
        let mut k = 0;
        if let Some(XNode::Text(lead)) = tag.children.first() {
            if js::trim(lead).is_empty() {
                k = 1;
            } else {
                let lead = lead.clone();
                let pieces = self.chunk_up_text(&lead);
                self.chunks.extend(pieces);
                tag.children.remove(0);
            }
        }

        while k < tag.children.len() {
            let XNode::Element(child) = &tag.children[k] else {
                k += 1;
                continue;
            };
            let tail = match tag.children.get(k + 1) {
                Some(XNode::Text(t)) => Some(t.clone()),
                _ => None,
            };
            let raw = ser(child).into_bytes();
            if raw.len() > CHUNK_SIZE && child.get_attr("aid").is_some() {
                if let XNode::Element(child) = &mut tag.children[k] {
                    self.step_into_tag(child)?;
                }
                match tail {
                    Some(t) if !js::trim(&t).is_empty() => {
                        let pieces = self.chunk_up_text(&t);
                        self.chunks.extend(pieces);
                        tag.children.remove(k + 1);
                        k += 1;
                    }
                    Some(_) => k += 2,
                    None => k += 1,
                }
            } else {
                let c = self.chunk(raw);
                self.chunks.push(c);
                let had_tail = tail.is_some();
                if let Some(t) = tail {
                    let pieces = self.chunk_up_text(&t);
                    self.chunks.extend(pieces);
                }
                tag.children.drain(k..k + if had_tail { 2 } else { 1 });
            }
        }

        if self.chunks.len() <= first && !self.chunks.is_empty() {
            return Err(Azw3Error::new(
                "Stepped into an element that made no fragments.",
            ));
        }
        if !self.chunks.is_empty() {
            self.chunks[first].starts_tags.push(aid.clone());
            self.chunks
                .last_mut()
                .expect("one chunk")
                .ends_tags
                .push(aid.clone());
        }
        self.selector = format!("S-//*[@aid='{aid}']");
        Ok(())
    }
}

/// Neighbouring small fragments of one element merge, up to the size limit
/// (skeleton.py's merge_small_chunks).
fn merge_small_chunks(chunks: Vec<Chunk>) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = Vec::new();
    for chunk in chunks {
        let Some(prev) = out.last_mut() else {
            out.push(chunk);
            continue;
        };
        if !chunk.starts_tags.is_empty()
            || chunk.raw.len() + prev.raw.len() > CHUNK_SIZE
            || !prev.ends_tags.is_empty()
        {
            out.push(chunk);
        } else {
            prev.raw.extend_from_slice(&chunk.raw);
            prev.ends_tags = chunk.ends_tags;
        }
    }
    out
}

struct Skeleton {
    skeleton: Vec<u8>,
    chunks: Vec<Chunk>,
    start_pos: usize,
}

/// skeleton.py's Skeleton: render what is left of the file, measure each
/// remaining aid element, and work out where each fragment goes back.
fn make_skeleton(root: &XElement, mut chunks: Vec<Chunk>) -> Result<Skeleton, Azw3Error> {
    let rendered = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}\n",
        ser(root).replacen("<html", &format!("<html xmlns=\"{XHTML_NS}\""), 1)
    );
    let body_offset = rendered
        .find("<body")
        .ok_or_else(|| Azw3Error::new("A skeleton has no <body>."))?;

    // For each aid element still in the skeleton: the bytes from its start
    // to where its content begins, and from where its content ends to the
    // end of its tail.
    let mut metrics: HashMap<String, (usize, usize)> = HashMap::new();
    fn walk(el: &XElement, metrics: &mut HashMap<String, (usize, usize)>) {
        for (i, c) in el.children.iter().enumerate() {
            let XNode::Element(c) = c else { continue };
            if let Some(aid) = c.get_attr("aid") {
                let lead = match c.children.first() {
                    Some(XNode::Text(t)) => escape_text(t).len(),
                    _ => 0,
                };
                let tail = match el.children.get(i + 1) {
                    Some(XNode::Text(t)) => escape_text(t).len(),
                    _ => 0,
                };
                metrics.insert(
                    aid.to_owned(),
                    (
                        open_tag(c).len() + 1 + lead,
                        format!("</{}>", c.name).len() + tail,
                    ),
                );
            }
            walk(c, metrics);
        }
    }
    walk(root, &mut metrics);

    let metric = |t: &str| {
        metrics
            .get(t)
            .copied()
            .ok_or_else(|| Azw3Error::new(format!("No aid {t} in the skeleton.")))
    };
    let mut pos = body_offset;
    for chunk in &mut chunks {
        for t in &chunk.starts_tags {
            pos += metric(t)?.0;
        }
        chunk.insert_pos = pos;
        pos += chunk.raw.len();
        for t in &chunk.ends_tags {
            pos += metric(t)?.1;
        }
    }
    Ok(Skeleton {
        skeleton: rendered.into_bytes(),
        chunks,
        start_pos: 0,
    })
}

fn rebuild(s: &Skeleton) -> Vec<u8> {
    let mut out = s.skeleton.clone();
    for c in &s.chunks {
        out.splice(c.insert_pos..c.insert_pos, c.raw.iter().copied());
    }
    out
}

fn skel_length(s: &Skeleton) -> usize {
    s.skeleton.len() + s.chunks.iter().map(|c| c.raw.len()).sum::<usize>()
}

/// `/<[^>]+? [ac]id=['"]([cA-Z0-9]+)['"]/g`.
static AID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<[^>]+? [ac]id=['"]([cA-Z0-9]+)['"]"#, "g"));
/// `/<[^>]+(kindle:pos:fid:0000:off:[0-9A-Za-z]{10})/g`.
static PLACEHOLDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+(kindle:pos:fid:0000:off:[0-9A-Za-z]{10})", "g"));

/// The path, child index by child index, to the first `body` in preorder.
fn path_to_body(el: &XElement, path: &mut Vec<usize>) -> bool {
    for (i, c) in el.children.iter().enumerate() {
        if let XNode::Element(e) = c {
            path.push(i);
            if e.name == "body" || path_to_body(e, path) {
                return true;
            }
            path.pop();
        }
    }
    false
}

fn element_at<'a>(root: &'a mut XElement, path: &[usize]) -> &'a mut XElement {
    let mut el = root;
    for &i in path {
        let XNode::Element(e) = &mut el.children[i] else {
            unreachable!("a path to an element")
        };
        el = e;
    }
    el
}

/// Turn the EPUB's spine into KF8 text and tables. `resources` maps a
/// picture's package href to its resource number and mime type. The book's
/// documents are changed in place.
pub fn kf8_markup(
    book: &mut EpubBook,
    resources: &HashMap<String, ResourceRef>,
) -> Result<Kf8Markup, Azw3Error> {
    let mut used_images: Vec<String> = Vec::new();
    let mut flows: Vec<Vec<u8>> = vec![Vec::new()];
    let spine_hrefs: HashSet<String> = book.spine.iter().map(|s| s.href.clone()).collect();

    // main.py's cleanup_markup: empty scripts go; any aid or cid already in
    // the markup goes, since aids are this writer's to assign.
    for s in &mut book.spine {
        walk_mut(&mut s.doc.root, &mut |el| {
            el.children.retain(|c| {
                !matches!(c, XNode::Element(e) if local_name(&e.name) == "script"
                    && e.children.is_empty() && e.get_attr("src").is_none())
            });
            el.remove_attr("aid");
            el.remove_attr("cid");
            Ok(())
        })?;
    }

    // replace_resource_links: pictures become kindle:embed references.
    for s in &mut book.spine {
        let base = dir_of(&s.href).to_owned();
        walk_mut(&mut s.doc.root, &mut |el| {
            let name = local_name(&el.name);
            if name != "img" && name != "image" {
                return Ok(());
            }
            for pair in &mut el.attrs {
                let attr = local_name(&pair.0).to_lowercase();
                if attr != "src" && attr != "href" {
                    continue;
                }
                let resolved = resolve_href(&base, &pair.1)?;
                let target = split_fragment(&resolved).0.to_owned();
                if let Some(r) = resources.get(&target) {
                    if !used_images.contains(&target) {
                        used_images.push(target);
                    }
                    pair.1 = format!("kindle:embed:{}?mime={}", to_ref(r.index), r.mime);
                }
            }
            Ok(())
        })?;
    }

    // extract_css_into_flows: each stylesheet is a flow, linked as
    // kindle:flow; an inline <style> becomes a flow of its own, once per
    // text.
    let mut sheets: HashMap<String, usize> = HashMap::new();
    for (href, css) in &book.stylesheets {
        sheets.insert(href.clone(), flows.len());
        flows.push(css.clone().into_bytes());
    }
    let mut inline: HashMap<String, usize> = HashMap::new();
    for s in &mut book.spine {
        let base = dir_of(&s.href).to_owned();
        walk_mut(&mut s.doc.root, &mut |el| {
            if local_name(&el.name) == "link"
                && let Some(href) = el.get_attr("href")
            {
                let resolved = resolve_href(&base, href)?;
                if let Some(&idx) = sheets.get(&resolved) {
                    el.set_attr(
                        "href",
                        &format!("kindle:flow:{}?mime=text/css", to_ref(idx)),
                    );
                }
            }
            let mut out = Vec::with_capacity(el.children.len());
            for c in std::mem::take(&mut el.children) {
                match c {
                    XNode::Element(e) if local_name(&e.name) == "style" => {
                        let raw = text_of(&e.children);
                        if js::trim(&raw).is_empty() {
                            continue;
                        }
                        let idx = *inline.entry(raw.clone()).or_insert_with(|| {
                            flows.push(raw.into_bytes());
                            flows.len() - 1
                        });
                        out.push(XNode::Element(XElement {
                            name: "link".to_owned(),
                            attrs: vec![
                                ("type".to_owned(), "text/css".to_owned()),
                                ("rel".to_owned(), "stylesheet".to_owned()),
                                (
                                    "href".to_owned(),
                                    format!("kindle:flow:{}?mime=text/css", to_ref(idx)),
                                ),
                            ],
                            children: Vec::new(),
                        }));
                        out.push(XNode::Text("\n".to_owned()));
                    }
                    c => out.push(c),
                }
            }
            el.children = out;
            Ok(())
        })?;
        merge_text(&mut s.doc.root);
    }

    // replace_internal_links_with_placeholders: a link to a spine document
    // gets a placeholder, filled in once the fragments are known.
    let mut link_map: HashMap<String, (String, String)> = HashMap::new();
    let mut count = 0usize;
    for s in &mut book.spine {
        let base = dir_of(&s.href).to_owned();
        walk_mut(&mut s.doc.root, &mut |el| {
            if local_name(&el.name) != "a" {
                return Ok(());
            }
            let Some(href) = el.get_attr("href") else {
                return Ok(());
            };
            count += 1;
            let resolved = resolve_href(&base, href)?;
            let (target, frag) = split_fragment(&resolved);
            if spine_hrefs.contains(target) {
                let placeholder = format!("kindle:pos:fid:0000:off:{}", to_href(count));
                link_map.insert(
                    placeholder.clone(),
                    (target.to_owned(), frag.unwrap_or("").to_owned()),
                );
                el.set_attr("href", &placeholder);
            }
            Ok(())
        })?;
    }

    // insert_aid_attributes. Inside a table only an element with an id gets
    // one, a cid ("The Kindle renderer barfs on large tables that have aid
    // on any of their tags", Calibre bug 1489495).
    let mut id_map: HashMap<(String, String), String> = HashMap::new();
    let mut cid = 0usize;
    for (i, s) in book.spine.iter_mut().enumerate() {
        let aid_base = i * 1_000_000;
        let mut j = 0usize;
        fn visit(
            el: &mut XElement,
            in_table: bool,
            href: &str,
            aid_base: usize,
            j: &mut usize,
            cid: &mut usize,
            id_map: &mut HashMap<(String, String), String>,
        ) {
            for c in &mut el.children {
                let XNode::Element(c) = c else { continue };
                let mut id = c.get_attr("id").map(str::to_owned);
                let tag_name = local_name(&c.name).to_lowercase();
                if id.is_none() && tag_name == "a" {
                    id = c.get_attr("name").map(str::to_owned);
                    if let Some(id) = &id {
                        c.set_attr("id", id);
                    }
                }
                let table = tag_name == "table" || in_table;
                if id.is_some() || aid_able(&tag_name) {
                    if table {
                        if let Some(id) = id.as_deref().filter(|i| !i.is_empty()) {
                            *cid += 1;
                            let val = format!("c{cid}");
                            id_map.insert((href.to_owned(), id.to_owned()), val.clone());
                            c.set_attr("cid", &val);
                        }
                    } else {
                        let aid = to_base((aid_base + *j) as u64, 32, 0);
                        c.set_attr("aid", &aid);
                        if tag_name == "body" {
                            id_map.insert((href.to_owned(), String::new()), aid.clone());
                        }
                        if let Some(id) = &id {
                            id_map.insert((href.to_owned(), id.clone()), aid);
                        }
                        *j += 1;
                    }
                }
                visit(c, table, href, aid_base, j, cid, id_map);
            }
        }
        visit(
            &mut s.doc.root,
            false,
            &s.href,
            aid_base,
            &mut j,
            &mut cid,
            &mut id_map,
        );
    }

    // The Chunker.
    let mut skeletons: Vec<Skeleton> = Vec::new();
    for s in &book.spine {
        let mut root = remove_namespaces(&s.doc.root);
        // kindlegen always puts the aid last.
        walk_mut(&mut root, &mut |el| {
            if let Some(aid) = el.get_attr("aid").map(str::to_owned) {
                el.remove_attr("aid");
                el.attrs.push(("aid".to_owned(), aid));
            }
            Ok(())
        })?;
        let mut path = Vec::new();
        if !path_to_body(&root, &mut path) {
            return Err(Azw3Error::new("A spine document has no <body>."));
        }
        // body.tail = '\n'
        let (&at, parent_path) = path.split_last().expect("body has a parent");
        let parent = element_at(&mut root, parent_path);
        match parent.children.get_mut(at + 1) {
            Some(XNode::Text(t)) => *t = "\n".to_owned(),
            _ => parent.children.insert(at + 1, XNode::Text("\n".to_owned())),
        }

        let mut chunker = Chunker {
            selector: String::new(),
            chunks: Vec::new(),
        };
        chunker.step_into_tag(element_at(&mut root, &path))?;
        skeletons.push(make_skeleton(&root, merge_small_chunks(chunker.chunks))?);
    }

    // create_tables.
    let mut skel_table = Vec::new();
    let mut chunk_table: Vec<ChunkEntry> = Vec::new();
    let mut sp = 0usize;
    for (file_number, s) in skeletons.iter_mut().enumerate() {
        s.start_pos = sp;
        sp += skel_length(s);
        skel_table.push(SkelEntry {
            file_number,
            chunk_count: s.chunks.len(),
            start_pos: s.start_pos,
            length: s.skeleton.len(),
        });
    }
    let mut seq = 0usize;
    for (file_number, s) in skeletons.iter().enumerate() {
        let mut cp = 0usize;
        for c in &s.chunks {
            chunk_table.push(ChunkEntry {
                insert_pos: c.insert_pos + s.start_pos,
                selector: c.selector.clone(),
                file_number,
                sequence_number: seq,
                start_pos: cp,
                length: c.raw.len(),
            });
            seq += 1;
            cp += c.raw.len();
        }
    }

    // set_internal_links: where each aid lands, from the REBUILT text, as a
    // fragment number and an offset into it.
    let mut stored = Vec::new();
    for s in &skeletons {
        stored.extend_from_slice(&s.skeleton);
        for c in &s.chunks {
            stored.extend_from_slice(&c.raw);
        }
    }
    let rebuilt_bytes: Vec<u8> = skeletons.iter().flat_map(rebuild).collect();
    let rebuilt = String::from_utf8_lossy(&rebuilt_bytes).into_owned();
    let chars: Vec<char> = rebuilt.chars().collect();
    let mut aid_offsets: HashMap<String, AidOffset> = HashMap::new();
    let mut byte_at = 0usize;
    let mut char_at = 0usize;
    let mut c = 0usize;
    let mut from = 0usize;
    while let Some(caps) = AID_RE.find_from(&chars, from) {
        let (a, b) = caps[0].expect("a match");
        from = if b == a { b + 1 } else { b };
        // Character index to byte offset, incrementally.
        byte_at += chars[char_at..a]
            .iter()
            .map(|ch| ch.len_utf8())
            .sum::<usize>();
        char_at = a;
        let offset = byte_at;
        while c + 1 < chunk_table.len()
            && chunk_table[c].insert_pos + chunk_table[c].length <= offset
        {
            c += 1;
        }
        let aid = group(&chars, &caps, 1).unwrap_or_default();
        let chunk = chunk_table
            .get(c)
            .ok_or_else(|| Azw3Error::new(format!("No fragment holds aid {aid}.")))?;
        let pos_fid = if chunk.insert_pos <= offset {
            (chunk.sequence_number, offset - chunk.insert_pos, offset)
        } else {
            (chunk.sequence_number, 0, offset) // in the skeleton: the fragment after it
        };
        aid_offsets.insert(aid, pos_fid);
    }

    // Fill the link placeholders in the stored text; same length, so
    // nothing moves.
    let mut text = stored;
    if !link_map.is_empty() {
        let s = String::from_utf8_lossy(&text).into_owned();
        let filled = PLACEHOLDER_RE.replace_with(&s, true, |chars, caps| {
            let (a, b) = caps[0].expect("a match");
            let raw: String = chars[a..b].iter().collect();
            let placeholder = group(chars, caps, 1).unwrap_or_default();
            let Some((target, frag)) = link_map.get(&placeholder) else {
                return raw;
            };
            let aid = id_map
                .get(&(target.clone(), frag.clone()))
                .or_else(|| id_map.get(&(target.clone(), String::new())));
            let Some(at) = aid.and_then(|aid| aid_offsets.get(aid)) else {
                return raw;
            };
            let keep: String = raw.chars().take(raw.chars().count() - 19).collect();
            format!("{keep}{}:off:{}", to_ref(at.0), to_href(at.1))
        });
        text = filled.into_bytes();
    }

    flows[0] = text;
    Ok(Kf8Markup {
        flows,
        skel_table,
        chunk_table,
        aid_offsets,
        id_map,
        used_images,
    })
}
