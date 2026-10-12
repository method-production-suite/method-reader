//! The book's pages, drawn natively from the engine's blocks (see
//! model/preview.rs and the design doc's first gap). One layout serves the
//! three places the window shows the book: the result's first page, the
//! Read surface and the Settings preview.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Color32, CornerRadius, FontFamily, FontId, Galley, Pos2, Rect, Stroke, StrokeKind, Ui,
    Vec2,
};

use crate::model::preview::{BlockKind, Face, Preview, PreviewStyle};
use crate::model::read::{Mark, PAGE_MARGIN_BLOCK_EM, PAGE_MARGIN_INLINE_PCT};
use crate::theme::{self, Tokens};

/// The book's base type size, in points.
pub const BOOK_SIZE: f32 = 16.0;

/// One block, laid out.
pub struct Placed {
    pub galley: Arc<Galley>,
    /// Relative to the top-left of the page's content box.
    pub pos: Pos2,
    pub scene: usize,
    pub block: usize,
    pub marker: Option<Arc<Galley>>,
    pub words: String,
}

pub struct Laid {
    pub blocks: Vec<Placed>,
    pub marks: Vec<Mark>,
    pub height: f32,
}

fn family(face: Face, bold: bool) -> FontFamily {
    FontFamily::Name(
        match (face, bold) {
            (Face::Courier, false) => theme::STRUCTURE,
            (Face::Courier, true) => theme::STRUCTURE_BOLD,
            (Face::Serif, _) => theme::PROSE,
            (Face::Sans, _) => "sans",
        }
        .into(),
    )
}

/// Lay the whole preview out at `width` (the page's full width, margins
/// included: 7% each side, 1.6em top and bottom, as a device would).
pub fn lay_out(ui: &Ui, t: &Tokens, preview: &Preview, width: f32) -> Laid {
    let style = &preview.style;
    let size = BOOK_SIZE;
    let inline = width * PAGE_MARGIN_INLINE_PCT / 100.0;
    let column = (width - 2.0 * inline).max(40.0);
    let gap = style.element_spacing_em * size;
    let mut y = PAGE_MARGIN_BLOCK_EM * size;
    let mut blocks = Vec::new();
    let mut marks = Vec::new();
    let mut last_speech: Option<u32> = None;
    for (si, scene) in preview.scenes.iter().enumerate() {
        let scene_top = y;
        for (bi, block) in scene.blocks.iter().enumerate() {
            let in_speech = block.speech.is_some() && block.speech == last_speech;
            let above = match block.kind {
                BlockKind::SceneHeading if !(si == 0 && bi == 0) => gap * 2.0,
                _ if si == 0 && bi == 0 => 0.0,
                _ if in_speech => 0.0,
                _ => gap,
            };
            last_speech = block.speech;
            y += above;
            let (left, right, align) = horizontal(style, &block.kind, column);
            let bold = matches!(block.kind, BlockKind::SceneHeading | BlockKind::MiniSlug);
            let mut job = LayoutJob::default();
            job.wrap.max_width = (column - left - right).max(20.0);
            job.halign = align;
            job.justify =
                style.justify && matches!(block.kind, BlockKind::Action | BlockKind::Dialogue);
            let words = match &block.kind {
                BlockKind::Figure { .. } => "Picture".to_string(),
                _ => block.words(),
            };
            for span in &block.spans {
                job.append(
                    &span.text,
                    0.0,
                    TextFormat {
                        font_id: FontId::new(size, family(style.face, bold || span.bold)),
                        color: t.ink,
                        italics: span.italic,
                        underline: if span.underline {
                            Stroke::new(1.0, t.ink)
                        } else {
                            Stroke::NONE
                        },
                        ..Default::default()
                    },
                );
            }
            if block.spans.is_empty() {
                job.append(
                    &words,
                    0.0,
                    TextFormat {
                        font_id: FontId::new(size, family(style.face, false)),
                        color: t.ink_muted,
                        ..Default::default()
                    },
                );
            }
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let x = inline
                + left
                + match align {
                    Align::Center => (column - left - right) / 2.0,
                    Align::Max => column - left - right,
                    Align::Min => 0.0,
                };
            let marker = block.page_marker.as_ref().map(|m| {
                ui.fonts_mut(|f| {
                    f.layout_no_wrap(
                        m.clone(),
                        FontId::new(size * 0.75, family(Face::Courier, false)),
                        t.ink_muted,
                    )
                })
            });
            let height = galley.size().y;
            blocks.push(Placed {
                galley,
                pos: Pos2::new(x, y),
                scene: si,
                block: bi,
                marker,
                words,
            });
            y += height;
        }
        marks.push(Mark {
            id: scene.id.clone(),
            top: scene_top,
            height: y - scene_top,
        });
    }
    y += PAGE_MARGIN_BLOCK_EM * size;
    Laid {
        blocks,
        marks,
        height: y,
    }
}

/// Where a block sits across the column: (left, right, alignment).
fn horizontal(style: &PreviewStyle, kind: &BlockKind, column: f32) -> (f32, f32, Align) {
    let pct = |p: f32| column * p / 100.0;
    let side = pct(style.dialogue_side_margin_pct);
    match kind {
        BlockKind::Character => match style.cue_indent_pct {
            Some(indent) => (pct(indent), 0.0, Align::Min),
            None => (0.0, 0.0, Align::Center),
        },
        BlockKind::Parenthetical => match style.cue_indent_pct {
            Some(_) => (pct(style.parenthetical_indent_pct), side, Align::Min),
            None => (side + pct(5.0), side + pct(5.0), Align::Min),
        },
        BlockKind::Dialogue => (side, side, Align::Min),
        BlockKind::Transition => (0.0, 0.0, Align::Max),
        BlockKind::Centered => (0.0, 0.0, Align::Center),
        _ => (0.0, 0.0, Align::Min),
    }
}

/// Paint laid-out blocks with `origin` as the page's top-left, those whose
/// index passes `keep`.
pub fn paint(
    ui: &Ui,
    laid: &Laid,
    origin: Pos2,
    clip: Rect,
    t: &Tokens,
    keep: impl Fn(usize) -> bool,
) {
    let painter = ui.painter().with_clip_rect(clip);
    for (i, placed) in laid.blocks.iter().enumerate() {
        if !keep(i) {
            continue;
        }
        let at = origin + placed.pos.to_vec2();
        let r = Rect::from_min_size(at, placed.galley.size());
        if !r.intersects(clip) {
            continue;
        }
        painter.galley(at, placed.galley.clone(), t.ink);
        if let Some(marker) = &placed.marker {
            let right = clip.right() - 8.0;
            painter.galley(
                Pos2::new(right - marker.size().x, at.y),
                marker.clone(),
                t.ink_muted,
            );
        }
    }
}

/// The page as a sheet of paper: the box the result's first page sits in.
pub fn sheet(ui: &Ui, t: &Tokens, rect: Rect) {
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::RADIUS_WELL as u8), t.paper);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::RADIUS_WELL as u8),
        Stroke::new(1.0, t.hole),
        StrokeKind::Inside,
    );
}

/// The result's first page: the book's own, opened on its first scene a
/// line and a half down; painted only (hidden from the screen reader, like
/// the frame it replaces). While the page writes itself, the typed page is
/// drawn over it.
pub fn first_page(
    ui: &Ui,
    t: &Tokens,
    rect: Rect,
    preview: Option<&Preview>,
    show: Option<&super::show::Show>,
) {
    sheet(ui, t, rect);
    let clip = rect.shrink(1.0);
    if let Some(preview) = preview {
        let laid = lay_out(ui, t, preview, rect.width());
        let first_scene = preview.scenes.iter().position(|s| {
            s.blocks
                .first()
                .is_some_and(|b| b.kind == BlockKind::SceneHeading)
        });
        let scene_top = first_scene
            .and_then(|i| laid.marks.get(i))
            .map(|m| m.top)
            .unwrap_or(0.0);
        let scroll = (scene_top - 1.5 * BOOK_SIZE).max(0.0);
        let typed_over = show.is_some_and(|s| s.covers_page());
        let fade = show.map(|s| s.page_opacity()).unwrap_or(0.0);
        if !typed_over || fade < 1.0 {
            paint(
                ui,
                &laid,
                rect.min - Vec2::new(0.0, scroll),
                clip,
                t,
                |_| true,
            );
        }
        if let Some(show) = show.filter(|s| s.covers_page()) {
            super::show::paint_typed(ui, t, rect, show, Some((preview, &laid)), fade);
        }
    } else if let Some(show) = show.filter(|s| s.covers_page()) {
        super::show::paint_typed(ui, t, rect, show, None, show.page_opacity());
    }
}

/// `a` moved `k` of the way to `b` (premultiplied, as Color32 stores it).
pub fn mix(a: Color32, b: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) * (1.0 - k) + f32::from(y) * k).round() as u8;
    Color32::from_rgba_premultiplied(
        ch(a.r(), b.r()),
        ch(a.g(), b.g()),
        ch(a.b(), b.b()),
        ch(a.a(), b.a()),
    )
}
