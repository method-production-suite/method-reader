//! The window's controls in the Method family's proportions (Method
//! Budget's `.btn`, `.btn-primary`, `.btn-quiet`: 28 points tall, a 1pt
//! rule border, a 5pt radius, the system sans at 13), its text styles, the
//! Beta badge and live lines, each with the name and role a screen reader
//! hears. Every word is drawn as written: sentence case, no caps.

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Response, RichText, Sense, Stroke, StrokeKind, Ui,
    Vec2, WidgetInfo, WidgetType, accesskit,
};

use crate::theme::{self, Tokens, size};

/// The family's control height, and the compact one.
pub const CONTROL_HEIGHT: f32 = 28.0;
pub const SMALL_HEIGHT: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// The primary action: the accent's fill (`.btn-primary`).
    Brass,
    /// A plain bordered button (`.btn`).
    Outline,
    /// A borderless one, ink on nothing until hovered (`.btn-quiet`).
    Quiet,
    /// A plain bordered button, compact.
    Small,
}

/// A button as the family's `.btn`, `.btn-primary` and `.btn-quiet` draw
/// it. Its accessible name is its own words as written.
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
    let (font, pad_x, height) = match look {
        Look::Brass => (theme::chrome_strong(size::UI), 12.0, CONTROL_HEIGHT),
        Look::Outline => (theme::chrome(size::UI), 12.0, CONTROL_HEIGHT),
        Look::Small => (theme::chrome(size::UI), 10.0, SMALL_HEIGHT),
        Look::Quiet => (theme::chrome(size::UI), 8.0, CONTROL_HEIGHT),
    };
    let shown = words.to_string();
    let galley = ui
        .painter()
        .layout_no_wrap(shown.clone(), font.clone(), t.ink);
    let max = ui.available_width().max(galley.size().x.min(120.0));
    let desired = Vec2::new(
        (galley.size().x + pad_x * 2.0).min(max),
        height.max(galley.size().y + 4.0),
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
        // `:disabled { opacity: 0.45 }`; a press darkens a little.
        let alpha = if !enabled {
            0.45
        } else if pressed {
            0.8
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
                if hovered { t.bar } else { t.panel },
                t.ink,
                Some(t.rule_strong.lerp_to_gamma(t.rule, 0.6)),
            ),
            Look::Quiet => (
                if hovered { t.bar } else { Color32::TRANSPARENT },
                t.ink,
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
                Stroke::new(1.0, line.gamma_multiply(alpha)),
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

/// Where the window keeps whether rings show (egui's per-window memory).
fn rings_id() -> Id {
    Id::new("sp-app focus rings shown")
}

/// Read this frame's input for the focus ring's mode (model/focus.rs
/// `rings_after`). Called once a frame, before anything is drawn.
pub fn note_ring_input(ctx: &egui::Context) {
    use crate::model::focus::{Input, RINGS_AT_START, rings_after};
    let inputs: Vec<Input> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key {
                    pressed: true,
                    modifiers,
                    ..
                } if !(modifiers.command
                    || modifiers.ctrl
                    || modifiers.alt
                    || modifiers.mac_cmd) =>
                {
                    Some(Input::Keyboard)
                }
                egui::Event::PointerButton { pressed: true, .. }
                | egui::Event::Touch {
                    phase: egui::TouchPhase::Start,
                    ..
                } => Some(Input::Pointer),
                _ => None,
            })
            .collect()
    });
    ctx.data_mut(|d| {
        let before = d.get_temp(rings_id()).unwrap_or(RINGS_AT_START);
        d.insert_temp(rings_id(), rings_after(before, inputs));
    });
}

/// Whether this focused control wears its ring now: focused, and the
/// keyboard is what was last used (CSS `:focus-visible`). The focus
/// itself, and the screen reader's, are egui's either way.
pub fn ring_shown(ui: &Ui, response: &Response) -> bool {
    response.has_focus()
        && ui
            .ctx()
            .data(|d| d.get_temp(rings_id()))
            .unwrap_or(crate::model::focus::RINGS_AT_START)
}

/// The 2px accent ring a focused control wears, after keyboard use only
/// (the family's `:focus-visible { outline: 2px solid var(--accent) }`).
pub fn focus_ring(ui: &Ui, t: &Tokens, response: &Response, offset: f32) {
    if ring_shown(ui, response) {
        ui.painter().rect_stroke(
            response.rect.expand(offset),
            CornerRadius::same(theme::RADIUS as u8),
            Stroke::new(2.0, t.brass),
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

/// A page or sheet's title: the system sans at 600, ink, sentence case.
pub fn title(ui: &mut Ui, t: &Tokens, words: &str) -> Response {
    heading(ui, words, theme::chrome_strong(size::BODY), t.ink)
}

/// Running text at the chrome's size.
pub fn prose(ui: &mut Ui, t: &Tokens, words: &str) -> Response {
    text(ui, words, theme::chrome(size::UI), t.ink_soft)
}

/// A caption under a component; alarm-coloured when `bad`.
pub fn caption(ui: &mut Ui, t: &Tokens, words: &str, bad: bool) -> Response {
    text(
        ui,
        words,
        theme::chrome(size::LABEL),
        if bad { t.alarm } else { t.ink_muted },
    )
}

/// A group's label: the system sans at 600, small, muted.
pub fn state_label(ui: &mut Ui, t: &Tokens, words: &str) -> Response {
    let r = text(ui, words, theme::chrome_strong(size::LABEL), t.ink_muted);
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

/// The one Beta badge: real text in a hairline box, in the family's caution
/// amber (never the accent), described by its note.
pub fn beta_badge(ui: &mut Ui, t: &Tokens) -> Response {
    let font = theme::chrome_strong(size::NOTE);
    let galley = ui
        .painter()
        .layout_no_wrap(BETA_LABEL.to_string(), font, t.note);
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + Vec2::new(10.0, 2.0), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::RADIUS_MARK as u8),
        Stroke::new(1.0, t.note),
        StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, t.note);
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

/// A card: the surface fill, a rule hairline, the well's corners.
pub fn panel_frame(t: &Tokens) -> egui::Frame {
    egui::Frame::NONE
        .fill(t.panel)
        .stroke(Stroke::new(1.0, t.rule))
        .corner_radius(CornerRadius::same(theme::RADIUS_WELL as u8))
        .inner_margin(egui::Margin::same(16))
}

/// A thin horizontal track and its fill (the conversion bar), 260 wide.
pub fn track(ui: &mut Ui, t: &Tokens, fraction: f32) -> egui::Rect {
    let width = 260.0f32.min(ui.available_width() * 0.7);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, 2.0, t.rule);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 2.0, t.brass);
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
