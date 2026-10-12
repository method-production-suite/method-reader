//! The window's controls: `surfaces.css`'s buttons (brass, outline,
//! quiet), its text styles, the Beta badge and live lines, each with the
//! name and role a screen reader hears.

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Response, RichText, Sense, Stroke, StrokeKind, Ui,
    Vec2, WidgetInfo, WidgetType, accesskit,
};

use crate::theme::{self, Tokens, size};

/// The CSS caps: the structure face set in upper case.
pub fn caps(text: &str) -> String {
    text.to_uppercase()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Brass,
    Outline,
    Quiet,
    Small,
}

/// A button as `.btn-brad`, `.btn-outline`, `.btn-quiet` drew it. Its
/// accessible name is its own words as written; the caps are paint.
pub fn button(ui: &mut Ui, t: &Tokens, look: Look, words: &str, enabled: bool) -> Response {
    draw_button(ui, t, look, words, enabled, None)
}

/// The same button under an id of the caller's, so it is the same widget
/// across redraws that move it (the send panel's keyed buttons).
pub fn button_id(
    ui: &mut Ui,
    t: &Tokens,
    look: Look,
    words: &str,
    enabled: bool,
    id: Id,
) -> Response {
    draw_button(ui, t, look, words, enabled, Some(id))
}

fn draw_button(
    ui: &mut Ui,
    t: &Tokens,
    look: Look,
    words: &str,
    enabled: bool,
    id: Option<Id>,
) -> Response {
    let (font, pad) = match look {
        Look::Brass | Look::Outline => (theme::structure_bold(size::UI), Vec2::new(30.0, 10.0)),
        Look::Small => (theme::structure_bold(size::LABEL), Vec2::new(18.0, 6.0)),
        Look::Quiet => (theme::structure_bold(size::NOTE), Vec2::new(0.0, 2.0)),
    };
    let shown = caps(words);
    let galley = ui
        .painter()
        .layout_no_wrap(shown.clone(), font.clone(), t.ink);
    let max = ui.available_width().max(galley.size().x.min(120.0));
    let desired = Vec2::new(
        (galley.size().x + pad.x * 2.0).min(max),
        galley.size().y + pad.y * 2.0,
    );
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = match id {
        None => ui.allocate_exact_size(desired, sense),
        Some(id) => {
            let rect = ui.allocate_space(desired).1;
            (rect, ui.interact(rect, id, sense))
        }
    };
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, words));
    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let pressed = enabled && response.is_pointer_button_down_on();
        let alpha = if !enabled {
            0.35
        } else if pressed {
            0.75
        } else {
            1.0
        };
        let (fill, ink, outline) = match look {
            Look::Brass => (
                if hovered { t.brass_highlight } else { t.brass },
                t.ink_on_brass,
                None,
            ),
            Look::Outline | Look::Small => (
                if hovered {
                    t.hole
                } else {
                    Color32::TRANSPARENT
                },
                t.ink,
                Some(t.ink),
            ),
            Look::Quiet => (
                Color32::TRANSPARENT,
                if hovered { t.ink } else { t.ink_muted },
                None,
            ),
        };
        let p = ui.painter();
        let radius = CornerRadius::same(theme::RADIUS as u8);
        p.rect_filled(rect, radius, fill.gamma_multiply(alpha));
        if let Some(line) = outline {
            p.rect_stroke(
                rect,
                radius,
                Stroke::new(1.2, line.gamma_multiply(alpha)),
                StrokeKind::Inside,
            );
        }
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            shown,
            font,
            ink.gamma_multiply(alpha),
        );
        focus_ring(ui, t, &response, 2.0);
    }
    response
}

/// The 2px ink ring a focused control wears.
pub fn focus_ring(ui: &Ui, t: &Tokens, response: &Response, offset: f32) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(offset),
            CornerRadius::same(theme::RADIUS as u8),
            Stroke::new(2.0, t.ink),
            StrokeKind::Outside,
        );
    }
}

/// Text in a given face, size and ink, wrapping anywhere it must.
pub fn text(ui: &mut Ui, words: &str, font: FontId, color: Color32) -> Response {
    ui.add(egui::Label::new(RichText::new(words).font(font).color(color)).wrap())
}

/// A heading the screen reader can jump to.
pub fn heading(ui: &mut Ui, words: &str, font: FontId, color: Color32) -> Response {
    let r = text(ui, words, font, color);
    ui.ctx().accesskit_node_builder(r.id, |n| {
        n.set_role(accesskit::Role::Heading);
        n.set_label(words);
    });
    r
}

/// A slugline: Courier bold caps at body size, ink. The heading's name is
/// its words as written.
pub fn slug(ui: &mut Ui, t: &Tokens, words: &str) -> Response {
    let r = text(ui, &caps(words), theme::structure_bold(size::BODY), t.ink);
    ui.ctx().accesskit_node_builder(r.id, |n| {
        n.set_role(accesskit::Role::Heading);
        n.set_label(words);
    });
    r
}

/// Running prose at body size.
pub fn prose(ui: &mut Ui, t: &Tokens, words: &str) -> Response {
    text(ui, words, theme::prose(size::BODY), t.ink_soft)
}

/// A caption under a component; alarm-coloured when `bad`.
pub fn caption(ui: &mut Ui, t: &Tokens, words: &str, bad: bool) -> Response {
    text(
        ui,
        words,
        theme::prose(size::CAPTION),
        if bad { t.alarm } else { t.ink_muted },
    )
}

/// A state label: small caps in the structure face, muted.
pub fn state_label(ui: &mut Ui, t: &Tokens, words: &str) -> Response {
    let r = text(ui, &caps(words), theme::structure(size::NOTE), t.ink_muted);
    ui.ctx()
        .accesskit_node_builder(r.id, |n| n.set_value(words));
    r
}

/// A line a screen reader announces when its words change (role=status,
/// aria-live polite). Drawn even when empty, so the region exists before
/// its first words arrive.
pub fn live(ui: &mut Ui, id: Id, words: &str, font: FontId, color: Color32) -> Response {
    let r = if words.is_empty() {
        ui.interact(
            egui::Rect::from_min_size(
                egui::emath::GuiRounding::round_to_pixels(ui.cursor().min, ui.pixels_per_point()),
                Vec2::ZERO,
            ),
            id,
            Sense::hover(),
        )
    } else {
        let label = ui.push_id(id, |ui| text(ui, words, font, color)).inner;
        ui.interact(label.rect, id, Sense::hover())
    };
    ui.ctx().accesskit_node_builder(r.id, |n| {
        n.set_role(accesskit::Role::Status);
        n.set_live(accesskit::Live::Polite);
        n.set_label(words);
    });
    r
}

pub const BETA_LABEL: &str = "Beta";
pub const BETA_NOTE: &str =
    "New: works in our tests, not yet tried widely on real devices or apps.";

/// The one Beta badge: real text in a hairline box, described by its note.
pub fn beta_badge(ui: &mut Ui, t: &Tokens) -> Response {
    let font = theme::structure(size::NOTE);
    let galley = ui
        .painter()
        .layout_no_wrap(caps(BETA_LABEL), font, t.ink_muted);
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + Vec2::new(8.0, 2.0), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::RADIUS_MARK as u8),
        Stroke::new(1.0, t.ink_muted),
        StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, t.ink_muted);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, BETA_LABEL));
    ui.ctx()
        .accesskit_node_builder(response.id, |n| n.set_description(BETA_NOTE));
    response.on_hover_text(BETA_NOTE)
}

/// Tie `control` to the sentence that explains it (aria-describedby).
pub fn described_by(ui: &Ui, control: Id, description: Id) {
    ui.ctx()
        .accesskit_node_builder(control, |n| n.push_described_by(description.accesskit_id()));
}

/// The arrow keys belong to this control while it has the focus (a tab
/// bar, a list, the script's stage), not to egui's own focus moving.
pub fn own_arrows(ui: &Ui, id: Id) {
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            egui::EventFilter {
                tab: false,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: false,
            },
        );
    });
}

/// A hairline box with the well's corners and the panel fill.
pub fn panel_frame(t: &Tokens) -> egui::Frame {
    egui::Frame::NONE
        .fill(t.panel)
        .stroke(Stroke::new(1.0, t.hole))
        .corner_radius(CornerRadius::same(theme::RADIUS_WELL as u8))
        .inner_margin(egui::Margin::same(19))
}

/// A thin horizontal track and its fill (the conversion bar), 220 wide.
pub fn track(ui: &mut Ui, t: &Tokens, fraction: f32) -> egui::Rect {
    let width = 220.0f32.min(ui.available_width() * 0.7);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 3.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t.hole);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 0.0, t.brass);
    rect
}

/// A rect as AccessKit takes it, for groups and regions egui itself does not
/// give bounds to.
pub fn bounds(r: egui::Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: r.min.x.into(),
        y0: r.min.y.into(),
        x1: r.max.x.into(),
        y1: r.max.y.into(),
    }
}
