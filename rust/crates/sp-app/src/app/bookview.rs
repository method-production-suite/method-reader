//! Where the window shows the book: Read, the Settings preview, the
//! result's first page and the page show. Each is a view of one [`Book`]
//! (the engine's own document laid out by Blitz), kept until the document,
//! the width or the theme changes, and painted only where it is on screen.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui::{
    CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, Vec2,
    WidgetInfo, WidgetType,
};

use super::book::Book;
use crate::model::read::Mark;
use crate::theme::{self, Tokens};

/// The book's base size, for the result's "a line and a half" above the
/// first scene (css.ts sets the body at 1em = 16px).
pub const BOOK_EM: f32 = 16.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    Read,
    Tune,
    FirstPage,
}

#[derive(PartialEq, Eq, Clone, Copy)]
struct Key {
    html: u64,
    width: u32,
    dark: bool,
}

pub struct View {
    key: Key,
    pub book: Book,
    slice: Option<(TextureHandle, i32, u32)>,
    blocks: HashMap<usize, TextureHandle>,
}

#[derive(Default)]
pub struct Views {
    views: HashMap<Place, View>,
}

fn hash(html: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    html.hash(&mut h);
    h.finish()
}

impl Views {
    /// The view of `html` at `width` in this theme, laid out afresh only
    /// when one of those changed.
    pub fn get(
        &mut self,
        place: Place,
        html: &str,
        width: f32,
        t: &Tokens,
        dark: bool,
    ) -> &mut View {
        let key = Key {
            html: hash(html),
            width: width.max(1.0).round() as u32,
            dark,
        };
        let stale = self.views.get(&place).is_none_or(|v| v.key != key);
        if stale {
            let book = Book::new(html, key.width, t, dark);
            self.views.insert(
                place,
                View {
                    key,
                    book,
                    slice: None,
                    blocks: HashMap::new(),
                },
            );
        }
        self.views.get_mut(&place).expect("just made")
    }

    pub fn peek(&self, place: Place) -> Option<&View> {
        self.views.get(&place)
    }

    pub fn forget(&mut self) {
        self.views.clear();
    }
}

impl View {
    pub fn marks(&self) -> Vec<Mark> {
        self.book.marks()
    }

    /// Paint the book's slice that starts `top` points down into `rect`.
    pub fn paint_slice(&mut self, ui: &Ui, id: &str, rect: Rect, top: f32) {
        let h = rect.height().max(1.0).round() as u32;
        let at = top.round() as i32;
        let fresh = !matches!(&self.slice, Some((_, t, hh)) if *t == at && *hh == h);
        if fresh {
            let image = self
                .book
                .paint_scaled(at as f32, h, ui.ctx().pixels_per_point());
            let tex = match self.slice.take() {
                Some((mut tex, _, _)) => {
                    tex.set(image, TextureOptions::LINEAR);
                    tex
                }
                None => ui.ctx().load_texture(id, image, TextureOptions::LINEAR),
            };
            self.slice = Some((tex, at, h));
        }
        if let Some((tex, _, _)) = &self.slice {
            let size = Vec2::new(self.book.width as f32, h as f32);
            ui.painter().with_clip_rect(rect).image(
                tex.id(),
                Rect::from_min_size(rect.min, size),
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }

    /// One block of the book as the book draws it (the page show's formed
    /// blocks), tinted toward `tint` while it glows.
    pub fn paint_block(
        &mut self,
        ui: &Ui,
        index: usize,
        at: egui::Pos2,
        clip: Rect,
        tint: egui::Color32,
    ) -> f32 {
        let Some(block) = self.book.blocks.get(index).cloned() else {
            return 0.0;
        };
        let h = block.rect.height().max(1.0).round() as u32;
        let tex = self.blocks.entry(index).or_insert_with(|| {
            let image = self
                .book
                .paint_scaled(block.rect.top(), h, ui.ctx().pixels_per_point());
            ui.ctx()
                .load_texture(format!("book-block-{index}"), image, TextureOptions::LINEAR)
        });
        let size = Vec2::new(self.book.width as f32, h as f32);
        ui.painter().with_clip_rect(clip).image(
            tex.id(),
            Rect::from_min_size(egui::pos2(clip.left(), at.y), size),
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            tint,
        );
        block.rect.height()
    }

    /// The book's words, block by block, for the screen reader: one node per
    /// heading and paragraph at its place, `origin` being the book's top-left
    /// on screen.
    pub fn speak(&self, ui: &Ui, salt: &str, origin: egui::Pos2, clip: Rect) {
        for (i, block) in self.book.blocks.iter().enumerate() {
            let r = block.rect.translate(origin.to_vec2());
            let node = ui.interact(
                r.intersect(clip.expand(4000.0)),
                Id::new((salt, "book-block", i)),
                Sense::hover(),
            );
            node.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &block.words));
        }
    }
}

/// The page as a sheet of paper: hairline, the well's corners, paper.
pub fn sheet(ui: &Ui, t: &Tokens, rect: Rect) {
    let r = CornerRadius::same(theme::RADIUS_WELL as u8);
    ui.painter().rect_filled(rect, r, t.paper);
    ui.painter()
        .rect_stroke(rect, r, Stroke::new(1.0, t.hole), StrokeKind::Inside);
}

/// `a` moved `k` of the way to `b` (premultiplied, as Color32 stores it).
pub fn mix(a: egui::Color32, b: egui::Color32, k: f32) -> egui::Color32 {
    let k = k.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) * (1.0 - k) + f32::from(y) * k).round() as u8;
    egui::Color32::from_rgba_premultiplied(
        ch(a.r(), b.r()),
        ch(a.g(), b.g()),
        ch(a.b(), b.b()),
        ch(a.a(), b.a()),
    )
}
