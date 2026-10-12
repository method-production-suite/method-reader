//! The book's pages, rendered from the ENGINE'S OWN preview document (the
//! EPUB's XHTML and css.ts) by Blitz, a pure-Rust HTML/CSS engine (Stylo,
//! Taffy, Parley, vello_cpu). The decision and its one shim are in
//! docs/superpowers/specs/2026-10-11-sp-app-design.md, "The book preview".
//!
//! As read.js did, the window adds only the device layer a reader adds over
//! a book: the theme's ink and paper and the page margins (1.6em, 7%). And
//! the page-marker shim, until Blitz places inline floats.

use std::sync::{Arc, OnceLock};

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use egui::{Color32, ColorImage, Rect, pos2, vec2};

use crate::model::read::Mark;
use crate::theme::Tokens;

/// Every face the window bundles, decoded once, for Parley.
fn faces() -> &'static Vec<Vec<u8>> {
    static FACES: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    FACES.get_or_init(|| {
        crate::fonts::FACES
            .iter()
            .filter_map(|(_, bytes)| wuff::decompress_woff2(bytes).ok())
            .collect()
    })
}

/// Pictures the engine wrote into the page as data: URIs, decoded at once;
/// every other address is refused, so the preview fetches nothing.
struct DataOnly;

impl NetProvider for DataOnly {
    fn fetch(&self, _doc: usize, request: Request, handler: Box<dyn NetHandler>) {
        if request.url.scheme() != "data" {
            return;
        }
        if let Ok(url) = data_url::DataUrl::process(request.url.as_str())
            && let Ok((bytes, _)) = url.decode_to_vec()
        {
            handler.bytes(request.url.to_string(), Bytes::from(bytes));
        }
    }
}

/// The page-marker shim (design doc): where a browser draws the float.
pub const MARKER_SHIM: &str = "h2.scene-heading, p { position: relative; } \
span.page-marker { float: none; position: absolute; right: 0; top: 0; }";

fn css_color(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// read.js's deviceCss: the theme and the margins, applied over the book.
pub fn device_css(t: &Tokens) -> String {
    format!(
        "html {{ color: {}; background: {}; }} body {{ padding: 1.6em 7%; }} {MARKER_SHIM}",
        css_color(t.ink),
        css_color(t.paper)
    )
}

/// One heading or paragraph, where it was laid out, and its words.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub words: String,
    pub rect: Rect,
}

pub struct Book {
    doc: HtmlDocument,
    pub width: u32,
    pub height: f32,
    /// The scenes (`section.scene`), their headings' words, where they are.
    pub scenes: Vec<(Mark, String)>,
    /// Every heading and paragraph (`h2, p`), in order.
    pub blocks: Vec<Block>,
}

impl Book {
    /// Lay the engine's document out at `width` points.
    pub fn new(html: &str, width: u32, t: &Tokens, dark: bool) -> Self {
        let style = format!("<style>{}</style></head>", device_css(t));
        let html = if html.contains("</head>") {
            html.replacen("</head>", &style, 1)
        } else {
            format!("{style}{html}")
        };
        let mut fonts = parley::FontContext::new();
        for face in faces() {
            fonts
                .collection
                .register_fonts(parley::fontique::Blob::new(Arc::new(face.clone())), None);
        }
        let scheme = if dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        };
        let mut doc = HtmlDocument::from_html(
            &html,
            DocumentConfig {
                viewport: Some(Viewport::new(width.max(1), 1000, 1.0, scheme)),
                font_ctx: Some(fonts),
                net_provider: Some(Arc::new(DataOnly)),
                ..Default::default()
            },
        );
        // Twice: the first pass asks for the pictures, the second lays them out.
        for _ in 0..2 {
            doc.as_mut().handle_messages();
            doc.as_mut().resolve(0.0);
        }
        let mut book = Self {
            doc,
            width,
            height: 0.0,
            scenes: vec![],
            blocks: vec![],
        };
        book.measure();
        book
    }

    fn measure(&mut self) {
        let base = self.doc.as_ref();
        self.height = base.root_element().final_layout().size.height;
        let rect_of = |id: blitz_dom::NodeId| {
            let node = base.get_node(id).expect("a node it listed");
            let at = node.absolute_position(0.0, 0.0);
            let size = node.final_layout().size;
            Rect::from_min_size(pos2(at.x, at.y), vec2(size.width, size.height))
        };
        // A heading's words without the page number inside it (read.js
        // headingText): every child but the marker.
        let words_of = |id: blitz_dom::NodeId, skip_marker: bool| -> String {
            let node = base.get_node(id).expect("a node it listed");
            if !skip_marker {
                return node.text_content();
            }
            node.children
                .iter()
                .filter_map(|c| base.get_node(*c))
                .filter(|c| {
                    !c.attr("class".into())
                        .is_some_and(|k| k.split_whitespace().any(|w| w == "page-marker"))
                })
                .map(|c| c.text_content())
                .collect()
        };
        self.scenes = base
            .query_selector_all("section.scene")
            .map(|ids| ids.into_iter().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|id| {
                let rect = rect_of(id);
                let node = base.get_node(id).expect("listed");
                let heading = base
                    .query_selector_all("section.scene > h2.scene-heading")
                    .unwrap_or_default()
                    .into_iter()
                    .find(|h| base.get_node(*h).and_then(|n| n.parent) == Some(id))
                    .map(|h| words_of(h, true))
                    .unwrap_or_default();
                let mark = Mark {
                    id: node.attr("id".into()).unwrap_or_default().to_string(),
                    top: rect.top(),
                    height: rect.height(),
                };
                (
                    mark,
                    heading.split_whitespace().collect::<Vec<_>>().join(" "),
                )
            })
            .collect();
        self.blocks = base
            .query_selector_all("h2, p")
            .unwrap_or_default()
            .into_iter()
            .map(|id| Block {
                words: words_of(id, true).trim().to_string(),
                rect: rect_of(id),
            })
            .filter(|b| !b.words.is_empty())
            .collect();
    }

    /// Paint the slice of the book from `top` down, `height` tall.
    pub fn paint(&mut self, top: f32, height: u32) -> ColorImage {
        self.paint_scaled(top, height, 1.0)
    }

    /// The same slice at `scale` device pixels per point (2 on a Retina
    /// screen), so the type is as sharp as the window's own.
    pub fn paint_scaled(&mut self, top: f32, height: u32, scale: f32) -> ColorImage {
        let scale = scale.clamp(1.0, 4.0);
        let w = ((self.width.max(1) as f32) * scale).round() as u32;
        let h = ((height.clamp(1, 8192) as f32) * scale)
            .round()
            .min(16_000.0) as u32;
        let base = self.doc.as_mut();
        base.set_viewport_scroll(blitz_dom::Point {
            x: 0.0,
            y: f64::from(top.max(0.0)),
        });
        let mut painter = anyrender_vello_cpu::VelloCpuScenePainter::new(w as u16, h as u16);
        blitz_paint::paint_scene(&mut painter, base, f64::from(scale), w, h, 0, 0);
        let pixmap = painter.finish();
        let pixels = pixmap
            .data()
            .iter()
            .map(|p| Color32::from_rgba_premultiplied(p.r, p.g, p.b, p.a))
            .collect();
        ColorImage::new([w as usize, h as usize], pixels)
    }

    /// The scenes as read.js's marks.
    pub fn marks(&self) -> Vec<Mark> {
        self.scenes.iter().map(|(m, _)| m.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{DARK, LIGHT};

    fn oracle(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/../../oracle/{name}/book.preview.html",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("an oracle preview")
    }

    fn px(img: &ColorImage, x: usize, y: usize) -> Color32 {
        img.pixels[y * img.size[0] + x]
    }

    #[test]
    fn the_engines_own_document_lays_out_with_its_scenes_and_blocks() {
        let html = oracle("field-station");
        let book = Book::new(&html, 706, &LIGHT, false);
        assert_eq!(
            book.scenes.len(),
            html.matches("<section class=\"scene\"").count()
        );
        assert!(
            book.height > 5000.0,
            "eighteen pages of script: {}",
            book.height
        );
        let tops: Vec<f32> = book.scenes.iter().map(|(m, _)| m.top).collect();
        assert!(
            tops.windows(2).all(|w| w[0] < w[1]),
            "scenes in order down the page"
        );
        // The heading's words, without the page number inside it.
        let heading = book
            .scenes
            .iter()
            .find(|(_, h)| h.starts_with("EXT. FIELD STATION"))
            .unwrap();
        assert_eq!(heading.1, "EXT. FIELD STATION - DAY");
    }

    #[test]
    fn percentage_margins_put_dialogue_in_its_column_and_cues_in_the_middle() {
        let book = Book::new(&oracle("field-station"), 706, &LIGHT, false);
        let action = book
            .blocks
            .iter()
            .find(|b| b.words.starts_with("MARA and DELACROIX come up"))
            .unwrap();
        let line = book.blocks.iter().find(|b| b.words == "You came.").unwrap();
        // body padding 7% each side: 49.4 of 706.
        assert!((action.rect.left() - 49.4).abs() < 1.0, "{:?}", action.rect);
        assert!(
            line.rect.left() > action.rect.left() + 100.0,
            "dialogue sits in its column"
        );
        assert!(line.rect.right() < action.rect.right() - 100.0);
    }

    #[test]
    fn dual_dialogue_sits_side_by_side() {
        let book = Book::new(&oracle("torture"), 706, &LIGHT, false);
        let left = book
            .blocks
            .iter()
            .find(|b| b.words.starts_with("LEFTMARK"))
            .unwrap();
        let right = book
            .blocks
            .iter()
            .find(|b| b.words.starts_with("RIGHTMARK"))
            .unwrap();
        assert!(
            (left.rect.top() - right.rect.top()).abs() < 2.0,
            "on one line"
        );
        assert!(right.rect.left() > left.rect.right(), "right of it");
    }

    #[test]
    fn a_page_marker_sits_at_the_right_edge_not_over_the_heading() {
        let mut book = Book::new(&oracle("images"), 706, &LIGHT, false);
        let heading = book
            .blocks
            .iter()
            .find(|b| b.words.starts_with("INT. ARCHIVE ROOM - PROJECTION"))
            .unwrap()
            .rect;
        let img = book.paint(heading.top() - 4.0, 40);
        let inked =
            |x0: usize, x1: usize| (x0..x1).any(|x| (0..40).any(|y| px(&img, x, y).r() < 150));
        assert!(inked(620, 660), "the marker, at the right edge");
        assert!(inked(48, 60), "the heading's first letter");
    }

    #[test]
    fn pictures_written_into_the_page_are_drawn() {
        let mut book = Book::new(&oracle("images"), 706, &LIGHT, false);
        let first = book.blocks[0].rect;
        let img = book.paint(first.bottom(), 200);
        let colourful = img
            .pixels
            .iter()
            .filter(|p| (i16::from(p.r()) - i16::from(p.g())).abs() > 60)
            .count();
        assert!(colourful > 1000, "the figure's pixels: {colourful}");
    }

    #[test]
    fn the_theme_is_the_device_layer() {
        let mut light = Book::new(&oracle("field-station"), 706, &LIGHT, false);
        let mut dark = Book::new(&oracle("field-station"), 706, &DARK, true);
        assert_eq!(px(&light.paint(0.0, 10), 2, 2), LIGHT.paper);
        assert_eq!(px(&dark.paint(0.0, 10), 2, 2), DARK.paper);
    }

    #[test]
    fn a_retina_slice_has_twice_the_pixels() {
        let mut book = Book::new(&oracle("field-station"), 706, &LIGHT, false);
        let img = book.paint_scaled(0.0, 100, 2.0);
        assert_eq!(img.size, [1412, 200]);
    }

    #[test]
    fn a_slice_further_down_is_a_different_page() {
        let mut book = Book::new(&oracle("field-station"), 706, &LIGHT, false);
        let top = book.paint(0.0, 300);
        let later = book.paint(3000.0, 300);
        assert_ne!(top.pixels, later.pixels);
    }
}
