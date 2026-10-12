//! The page on the desk: the paper, the binding's brads and punched hole,
//! the tab bar, the stamp and the update label, the release notes sheet,
//! the Kindle heads-up, the drag strip, and the window's keyboard and drops
//! (`desktop/ui/frame.js`, `main.js`, `heads-up.js`, `notes-surface.js`).

use egui::epaint::{Mesh, Vertex};
use egui::{
    Align, Color32, CornerRadius, Id, Key, Layout, Modifiers, Pos2, Rect, Sense, Stroke, Ui, Vec2,
    ViewportCommand, WidgetInfo, WidgetType, accesskit,
};

use super::widgets::{self, Look};
use super::{App, convert, panel, read, tune};
use crate::model::device::{
    HEADS_UP_COPY, HEADS_UP_LATER, HEADS_UP_MS, heads_up_for, heads_up_line,
};
use crate::model::frame::{Surface, TabKey, rev_label, tab_presence, tab_step};
use crate::model::layout;
use crate::model::notes::{RELEASE, visible_sections};
use crate::model::send_routes::Device;
use crate::model::update::{
    self as u, CheckOutcome, Drawn, Phase, label_actionable, notes_view, read_state,
    remember_answer, remembered_offer, run_check, update_label, updates_possible,
};
use crate::theme::{self, Tokens, size};

pub fn tab_id(s: Surface) -> Id {
    Id::new(("tab", s.label()))
}

// --------------------------------------------------------------- updates

#[derive(Default)]
pub struct UpdateState {
    pub phase: Option<Phase>,
    pub drawn: Drawn,
    pub notes_line: Option<String>,
}

impl UpdateState {
    pub fn usable(&self, host: &super::Host) -> bool {
        host.updater.ready() && updates_possible(host.platform)
    }

    /// At launch: what an earlier check found, then today's check.
    pub fn boot(&mut self, host: &mut super::Host) {
        if !self.usable(host) {
            return;
        }
        match remembered_offer(host.storage.as_ref(), RELEASE.version) {
            Some(version) => self.set(Some(Phase::Offer {
                version,
                body: String::new(),
                retrying: false,
            })),
            None => u::forget_found(host.storage.as_mut()),
        }
        self.check(host, false);
    }

    pub fn answer(&mut self, host: &mut super::Host, on: bool) {
        remember_answer(host.storage.as_mut(), on);
        if on && self.usable(host) {
            self.check(host, false);
        }
    }

    pub fn check(&mut self, host: &mut super::Host, manual: bool) -> CheckOutcome {
        let now = (host.clock)();
        let updater = &host.updater;
        let outcome = run_check(manual, host.storage.as_mut(), now, || updater.check());
        match &outcome {
            CheckOutcome::Offer { version, body } => {
                self.set(Some(Phase::Offer {
                    version: version.clone(),
                    body: body.clone(),
                    retrying: false,
                }));
            }
            CheckOutcome::Current { .. } => self.set(None),
            CheckOutcome::Error { message } if manual => self.notes_line = Some(message.clone()),
            _ => {}
        }
        outcome
    }

    fn set(&mut self, phase: Option<Phase>) {
        if let Some(p) = &phase {
            self.drawn.drew_offer = true;
            if let Some(v) = p.version() {
                self.drawn.version = Some(v.to_string());
            }
            if let Phase::Offer { body, .. } | Phase::Downloading { body, .. } = p {
                self.drawn.body = body.clone();
            }
        }
        self.phase = phase;
        self.notes_line = None;
    }
}

// --------------------------------------------------------------- heads-up

#[derive(Default)]
pub struct HeadsUp {
    pub seen: Option<Vec<String>>,
    pub offered: Option<Device>,
    pub until: Option<f64>,
}

impl HeadsUp {
    pub fn close(&mut self) {
        self.offered = None;
        self.until = None;
    }

    pub fn reset(&mut self) {
        self.close();
        self.seen = None;
    }
}

fn on_result(app: &App) -> bool {
    app.surface == Surface::Convert && app.convert.view == convert::View::Done
}

pub fn heads_up_heard(app: &mut App, devices: &[Device]) {
    let open = app.script.is_some() && !app.convert.converting;
    let (seen, show) = heads_up_for(app.heads_up.seen.as_deref(), devices, open, on_result(app));
    app.heads_up.seen = Some(seen);
    if let Some(device) = show {
        app.heads_up.offered = Some(device);
        app.heads_up.until = Some(app.now() + HEADS_UP_MS);
    }
}

pub fn heads_up_tick(app: &mut App, now: f64) {
    if app.heads_up.until.is_some_and(|u| now >= u) {
        app.heads_up.close();
    }
}

pub fn heads_up_recheck(app: &mut App) {
    if app.heads_up.offered.is_some() && (on_result(app) || app.convert.converting) {
        app.heads_up.close();
    }
}

// ------------------------------------------------------------------ input

/// Cmd/Ctrl-O, a file dropped or dragged over, and a key or click ending
/// the page show.
pub fn input(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let (open, dropped, hovering, pressed) = ctx.input_mut(|i| {
        let open = i.consume_key(Modifiers::COMMAND, Key::O);
        let dropped: Vec<String> = i
            .raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_string_lossy().into_owned())
            .filter(|p| !p.is_empty())
            .collect();
        let pressed = i
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
            || i.pointer.any_pressed();
        (open, dropped, !i.raw.hovered_files.is_empty(), pressed)
    });
    app.drag_hover = hovering;
    // Back from another app (perhaps from installing Kindle Previewer).
    let focused = ctx.input(|i| i.focused);
    if focused && !app.window_focused {
        panel::focus_returned(app);
    }
    app.window_focused = focused;
    let now = app.now();
    if pressed && let Some(show) = app.show.as_mut() {
        show.press(now);
    }
    if open {
        convert::choose(app);
    }
    if !dropped.is_empty() {
        convert::drop_paths(app, &dropped);
    }
    if let Some(show) = app.show.as_mut() {
        show.tick(now);
        if !show.running() && show.book_arrived() {
            app.show = None;
        }
    }
}

// ---------------------------------------------------------------- drawing

pub fn draw(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let window = ui.ctx().content_rect().size();
    let block = layout::block(window.x);
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(t.paper))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let top = ui.cursor().min;
                    // The drag strip: as tall as the sheet's top padding, part of
                    // the page, so it scrolls away and never covers a control.
                    let strip = Rect::from_min_size(top, Vec2::new(window.x, layout::SPACE[7]));
                    let drag = ui.interact(strip, Id::new("drag-strip"), Sense::drag());
                    if drag.drag_started() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    ui.add_space(layout::SPACE[7]);
                    let min_height = (window.y - layout::SPACE[7]).max(0.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(block.left);
                        ui.vertical(|ui| {
                            ui.set_width(block.width);
                            ui.set_min_height(min_height);
                            sheet(app, ui, block, window);
                        });
                    });
                });
        });
    paint_rail(ui, &t, window);
    notes_sheet(app, ui);
}

fn sheet(app: &mut App, ui: &mut Ui, block: layout::Span, window: Vec2) {
    let t = app.tokens();
    let sheet_top = ui.cursor().min.y;
    tab_bar(app, ui);
    ui.add_space(layout::SPACE[6]);
    heads_up(app, ui);
    // The pane: a focusable tabpanel, labelled by its tab, so Tab can leave
    // the bar and land in the surface before it has a control.
    let pane_id = Id::new(("surface", app.surface.label()));
    app.pane_id = pane_id;
    let pane = ui.interact(
        Rect::from_min_size(ui.cursor().min, Vec2::new(1.0, 1.0)),
        pane_id,
        Sense::focusable_noninteractive(),
    );
    ui.ctx().accesskit_node_builder(pane.id, |n| {
        n.set_role(accesskit::Role::TabPanel);
        n.set_label(app.surface.label());
        n.push_labelled_by(tab_id(app.surface).accesskit_id());
    });
    match app.surface {
        Surface::Convert => convert::draw(app, ui, block, window),
        Surface::Read => read::draw(app, ui, block, window),
        Surface::Tune => tune::draw(app, ui, block, window),
    }
    // The foot: the update label beside the rev stamp, the dead-engine line
    // above them, at the block's right edge.
    ui.add_space(layout::SPACE[10]);
    let min_bottom = sheet_top + window.y - layout::SPACE[7] - layout::SPACE[10];
    if ui.cursor().min.y < min_bottom {
        ui.add_space(min_bottom - ui.cursor().min.y);
    }
    if let Some(fault) = app.engine_fault.clone() {
        ui.with_layout(Layout::top_down(Align::Max), |ui| {
            ui.set_max_width(block.width.min(66.0 * 8.0));
            widgets::text(ui, &fault, theme::structure(size::NOTE), t.alarm);
        });
    }
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = layout::SPACE[4];
        let stamp = stamp_button(ui, &t, &rev_label(RELEASE.version));
        if stamp.clicked() {
            app.notes_open = true;
        }
        if let Some(words) = update_label(app.update.phase.as_ref()) {
            let actionable = label_actionable(app.update.phase.as_ref());
            let label = update_button(ui, &t, &words, actionable);
            if label.clicked() && actionable {
                // The transport is wave 3's: a click names what it would do.
                app.update.notes_line = Some(words);
            }
        }
    });
    ui.add_space(layout::SPACE[5]);
}

fn tab_bar(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let now = app.now();
    let reduced = app.reduced_motion();
    let bar = ui.horizontal(|ui| {
        // The gaps between tabs move the window, as the strip does:
        // registered first, so every tab sits above it.
        let gaps = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 20.0));
        let gap = ui.interact(gaps, Id::new("tab-gaps"), Sense::drag());
        if gap.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        ui.spacing_mut().item_spacing.x = layout::SPACE[5];
        for (index, s) in Surface::ALL.into_iter().enumerate() {
            let presence = tab_presence(app.tab_available(s), s == app.surface);
            if !presence.shown {
                continue;
            }
            // Arriving: fade and rise over 0.2 s, staggered by position.
            let arrival = app.arriving.get(&s).map(|at| {
                let delay = if index >= 2 {
                    70.0 * (index - 1) as f64
                } else {
                    0.0
                };
                ((now - at - delay) / 200.0).clamp(0.0, 1.0) as f32
            });
            let k = if reduced { 1.0 } else { arrival.unwrap_or(1.0) };
            let font = theme::structure_bold(size::NOTE);
            let words = widgets::caps(s.label());
            let galley = ui.painter().layout_no_wrap(words, font, t.ink);
            let size = galley.size() + Vec2::new(0.0, layout::SPACE[1] + 2.0);
            let sense = if presence.tab_stop {
                Sense::click()
            } else {
                Sense::CLICK
            };
            let rect = ui.allocate_space(size).1;
            let response = ui.interact(rect, tab_id(s), sense);
            let selected = s == app.surface;
            response.widget_info(|| {
                WidgetInfo::selected(WidgetType::Button, true, selected, s.label())
            });
            ui.ctx().accesskit_node_builder(response.id, |n| {
                n.set_role(accesskit::Role::Tab);
                n.set_selected(selected);
                n.clear_toggled();
            });
            let ink = if selected || response.hovered() {
                t.ink
            } else {
                t.ink_muted
            };
            let offset = Vec2::new(0.0, (1.0 - k) * -5.0);
            ui.painter()
                .galley(rect.min + offset, galley, ink.gamma_multiply(k));
            if selected {
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() - 0.75,
                    Stroke::new(1.5, t.brass),
                );
            }
            widgets::focus_ring(ui, &t, &response, 3.0);
            if response.clicked() {
                app.go_to(s);
            }
            widgets::own_arrows(ui, response.id);
            if response.has_focus() {
                let key = ui.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::NONE, Key::ArrowRight) {
                        Some(TabKey::Right)
                    } else if i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) {
                        Some(TabKey::Left)
                    } else if i.consume_key(egui::Modifiers::NONE, Key::Home) {
                        Some(TabKey::Home)
                    } else if i.consume_key(egui::Modifiers::NONE, Key::End) {
                        Some(TabKey::End)
                    } else {
                        None
                    }
                });
                if let Some(key) = key
                    && let Some(next) = tab_step(key, app.surface, |x| app.tab_available(x))
                {
                    app.go_to(next);
                    app.restore_focus = false;
                    ui.memory_mut(|m| m.request_focus(tab_id(next)));
                }
            }
        }
    });
    ui.ctx().accesskit_node_builder(bar.response.id, |n| {
        n.set_role(accesskit::Role::TabList);
        n.set_bounds(widgets::bounds(bar.response.rect));
        n.set_label("Screepub");
    });
}

/// The rev stamp: a printer's mark at the foot of the page, and a button.
fn stamp_button(ui: &mut Ui, t: &Tokens, words: &str) -> egui::Response {
    let font = theme::structure(size::FINE);
    let galley = ui
        .painter()
        .layout_no_wrap(widgets::caps(words), font, t.ink_muted);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, words));
    let ink = if response.hovered() {
        t.ink
    } else {
        t.ink_muted
    };
    ui.painter()
        .galley_with_override_text_color(rect.min, galley, ink);
    widgets::focus_ring(ui, t, &response, 3.0);
    response
}

/// The update label: the stamp's type in ink, a brass rule under it; inert
/// moments are disabled to the screen reader and take no pointer.
fn update_button(ui: &mut Ui, t: &Tokens, words: &str, actionable: bool) -> egui::Response {
    let font = theme::structure(size::FINE);
    let galley = ui
        .painter()
        .layout_no_wrap(widgets::caps(words), font, t.ink);
    let size = galley.size() + Vec2::new(0.0, layout::SPACE[1]);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if actionable {
            Sense::click()
        } else {
            Sense::focusable_noninteractive()
        },
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, actionable, words));
    ui.painter().galley(rect.min, galley, t.ink);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.75,
        Stroke::new(1.5, t.brass),
    );
    widgets::focus_ring(ui, t, &response, 3.0);
    response
}

fn heads_up(app: &mut App, ui: &mut Ui) {
    let Some(device) = app.heads_up.offered.clone() else {
        return;
    };
    let t = app.tokens();
    let title = app
        .script
        .as_ref()
        .map(|s| s.title.clone())
        .unwrap_or_default();
    widgets::panel_frame(&t)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::live(
                    ui,
                    Id::new("heads-up"),
                    &heads_up_line(&title),
                    theme::structure(size::UI),
                    t.ink,
                );
                let copy = widgets::button(ui, &t, Look::Brass, HEADS_UP_COPY, true);
                let later = widgets::button(ui, &t, Look::Quiet, HEADS_UP_LATER, true);
                if copy.clicked() {
                    app.heads_up.close();
                    app.restore_focus = true;
                    if !app.convert.converting {
                        app.go_to(Surface::Convert);
                        convert::show_result(app);
                        panel::copy_to(app, device.clone());
                    }
                }
                if later.clicked() {
                    app.heads_up.close();
                    app.restore_focus = true;
                }
            });
        });
    ui.add_space(layout::SPACE[4]);
}

// -------------------------------------------------------------- the notes

fn notes_sheet(app: &mut App, ui: &mut Ui) {
    if !app.notes_open {
        return;
    }
    let t = app.tokens();
    let ctx = ui.ctx().clone();
    let modal = egui::Modal::new(Id::new("release-notes"))
        .frame(
            egui::Frame::NONE
                .fill(t.paper)
                .corner_radius(CornerRadius::same(theme::RADIUS_WELL as u8))
                .inner_margin(egui::Margin::symmetric(32, 19)),
        )
        .backdrop_color(Color32::from_black_alpha(107))
        .show(&ctx, |ui| {
            let width = (44.0 * 16.0f32).min(ctx.content_rect().width() * 0.88);
            ui.set_width(width);
            ui.ctx().accesskit_node_builder(ui.id(), |n| {
                n.set_role(accesskit::Role::Dialog);
                n.set_label("Release notes");
            });
            let mut close = false;
            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                close = widgets::button(ui, &t, Look::Quiet, "Close", true).clicked();
            });
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.82 - 80.0)
                .show(ui, |ui| {
                    widgets::slug(ui, &t, &format!("Screepub {}", RELEASE.version));
                    widgets::prose(ui, &t, RELEASE.headline);
                    for section in visible_sections(RELEASE.sections) {
                        ui.add_space(layout::SPACE[6]);
                        let h = widgets::state_label(ui, &t, section.title);
                        ui.ctx()
                            .accesskit_node_builder(h.id, |n| n.set_role(accesskit::Role::Heading));
                        for item in section.items {
                            let mut job = egui::text::LayoutJob::default();
                            if !item.lead.is_empty() {
                                job.append(
                                    item.lead,
                                    0.0,
                                    egui::TextFormat {
                                        font_id: theme::structure_bold(size::UI),
                                        color: t.ink,
                                        ..Default::default()
                                    },
                                );
                                job.append(
                                    " ",
                                    0.0,
                                    egui::TextFormat {
                                        font_id: theme::prose(size::BODY),
                                        color: t.ink_soft,
                                        ..Default::default()
                                    },
                                );
                            }
                            job.append(
                                item.body,
                                0.0,
                                egui::TextFormat {
                                    font_id: theme::prose(size::BODY),
                                    color: t.ink_soft,
                                    ..Default::default()
                                },
                            );
                            ui.add(egui::Label::new(job).wrap());
                        }
                    }
                    updates_block(app, ui, &t);
                });
            close
        });
    if modal.inner || modal.should_close() {
        app.notes_open = false;
        app.restore_focus = true;
    }
}

fn updates_block(app: &mut App, ui: &mut Ui, t: &Tokens) {
    ui.add_space(layout::SPACE[6]);
    if !app.update.usable(&app.host) {
        widgets::caption(ui, t, u::NOT_HERE, false);
        return;
    }
    widgets::state_label(ui, t, "Updates");
    let mut on = read_state(app.host.storage.as_ref()).opted_in;
    let box_ = ui.checkbox(&mut on, u::AUTO_LABEL);
    if box_.changed() {
        app.update.answer(&mut app.host, on);
        app.update.notes_line = Some(if on { u::AUTO_ON } else { u::AUTO_OFF }.into());
    }
    let view = notes_view(
        app.update.phase.as_ref(),
        &app.update.drawn,
        RELEASE.version,
    );
    let check = widgets::button(ui, t, Look::Small, u::CHECK_BUTTON, !view.check_disabled);
    if check.clicked() {
        app.update.drawn.drew_offer = true;
        let outcome = app.update.check(&mut app.host, true);
        if let CheckOutcome::Error { message } = outcome {
            app.update.notes_line = Some(message);
        }
    }
    let view = notes_view(
        app.update.phase.as_ref(),
        &app.update.drawn,
        RELEASE.version,
    );
    let said = app
        .update
        .notes_line
        .clone()
        .or(view.say)
        .unwrap_or_default();
    widgets::live(
        ui,
        Id::new("update-status"),
        &said,
        theme::prose(size::CAPTION),
        t.ink_muted,
    );
    if let Some(body) = view.body {
        widgets::prose(ui, t, &body);
    }
    if let Some((words, disabled)) = view.install {
        let b = widgets::button(ui, t, Look::Brass, &words, !disabled);
        if b.clicked() {
            app.update.notes_line = Some(words);
        }
    }
}

// --------------------------------------------------------------- the rail

/// Two brads and the punched hole, fixed to the window; painted, hidden
/// from assistive technology.
fn paint_rail(ui: &Ui, t: &Tokens, window: Vec2) {
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("rail")));
    for f in layout::rail(window.x, window.y) {
        let rect = Rect::from_center_size(Pos2::new(f.x, f.y), Vec2::splat(f.size));
        if f.punch {
            punch(&painter, t, rect);
        } else {
            brad(&painter, t, rect);
        }
    }
}

/// An SVG radialGradient in objectBoundingBox units, focal point included.
struct Grad<'a> {
    cx: f32,
    cy: f32,
    r: f32,
    fx: f32,
    fy: f32,
    stops: &'a [(f32, Color32)],
}

fn stop_color(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mut prev = stops[0];
    if t <= prev.0 {
        return prev.1;
    }
    for &next in &stops[1..] {
        if t <= next.0 {
            let k = (t - prev.0) / (next.0 - prev.0).max(1e-6);
            return super::bookview::mix(prev.1, next.1, k);
        }
        prev = next;
    }
    prev.1
}

/// Fill an ellipse (centre, radii, rotation in degrees, all in the
/// symbol's 100-unit box mapped onto `rect`) with a flat colour or a
/// radial gradient.
fn ellipse(
    painter: &egui::Painter,
    rect: Rect,
    shape: [f32; 5],
    fill: Color32,
    grad: Option<&Grad>,
) {
    let [cx, cy, rx, ry, rot] = shape;
    let c = Pos2::new(cx, cy);
    let map = |p: Pos2| rect.min + p.to_vec2() * (rect.width() / 100.0);
    let (sin, cos) = rot.to_radians().sin_cos();
    let at = |u: f32, v: f32| Pos2::new(c.x + u * cos - v * sin, c.y + u * sin + v * cos);
    let color_at = |u: f32, v: f32| -> Color32 {
        let Some(g) = grad else { return fill };
        // In the ellipse's own bounding box, before rotation.
        let p = Vec2::new((u + rx) / (2.0 * rx), (v + ry) / (2.0 * ry));
        let (f, ce) = (Vec2::new(g.fx, g.fy), Vec2::new(g.cx, g.cy));
        let d = ce - f;
        let q = p - f;
        let a = d.length_sq() - g.r * g.r;
        let qd = q.dot(d);
        let disc = (qd * qd - a * q.length_sq()).max(0.0);
        let t = if a.abs() < 1e-6 {
            0.0
        } else {
            (qd - disc.sqrt()) / a
        };
        stop_color(g.stops, t)
    };
    let rings = 14;
    let segments = 48;
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: map(at(0.0, 0.0)),
        uv: egui::epaint::WHITE_UV,
        color: color_at(0.0, 0.0),
    });
    for ring in 1..=rings {
        let k = ring as f32 / rings as f32;
        for s in 0..segments {
            let a = s as f32 / segments as f32 * std::f32::consts::TAU;
            let (u, v) = (a.cos() * rx * k, a.sin() * ry * k);
            mesh.vertices.push(Vertex {
                pos: map(at(u, v)),
                uv: egui::epaint::WHITE_UV,
                color: color_at(u, v),
            });
        }
    }
    let idx = |ring: u32, s: u32| 1 + (ring - 1) * segments + (s % segments);
    for s in 0..segments {
        mesh.add_triangle(0, idx(1, s), idx(1, s + 1));
    }
    for ring in 1..rings as u32 {
        for s in 0..segments {
            mesh.add_triangle(idx(ring, s), idx(ring + 1, s), idx(ring + 1, s + 1));
            mesh.add_triangle(idx(ring, s), idx(ring + 1, s + 1), idx(ring, s + 1));
        }
    }
    painter.add(mesh);
}

/// brand/components/brad.html, layer by layer.
fn brad(painter: &egui::Painter, t: &Tokens, rect: Rect) {
    let black = |a: f32| Color32::from_black_alpha((a * 255.0) as u8);
    let white = |a: f32| Color32::from_white_alpha((a * 255.0) as u8);
    let cast = Grad {
        cx: 0.5,
        cy: 0.5,
        r: 0.5,
        fx: 0.5,
        fy: 0.5,
        stops: &[(0.52, black(0.36)), (1.0, black(0.0))],
    };
    ellipse(
        painter,
        rect,
        [53.0, 56.0, 47.0, 46.0, 0.0],
        Color32::TRANSPARENT,
        Some(&cast),
    );
    ellipse(
        painter,
        rect,
        [50.0, 50.0, 41.0, 41.0, 0.0],
        t.brass_edge,
        None,
    );
    let face_stops = [
        (0.0, t.brass_specular),
        (0.17, t.brass_highlight),
        (0.45, t.brass),
        (0.76, t.brass_shadow),
        (1.0, t.brass_rim),
    ];
    let face = Grad {
        cx: 0.5,
        cy: 0.5,
        r: 0.58,
        fx: 0.33,
        fy: 0.27,
        stops: &face_stops,
    };
    ellipse(
        painter,
        rect,
        [50.0, 50.0, 39.5, 39.5, 0.0],
        t.brass,
        Some(&face),
    );
    ellipse(
        painter,
        rect,
        [63.0, 67.0, 15.0, 8.0, -28.0],
        t.brass_bounce.gamma_multiply(0.34),
        None,
    );
    let spec = Grad {
        cx: 0.5,
        cy: 0.5,
        r: 0.5,
        fx: 0.5,
        fy: 0.5,
        stops: &[(0.0, white(0.88)), (0.55, white(0.2)), (1.0, white(0.0))],
    };
    ellipse(
        painter,
        rect,
        [38.0, 33.0, 17.0, 11.0, -28.0],
        Color32::TRANSPARENT,
        Some(&spec),
    );
    painter.circle_stroke(
        rect.center(),
        39.5 * rect.width() / 100.0,
        Stroke::new(
            1.6 * rect.width() / 100.0,
            t.brass_edge.gamma_multiply(0.55),
        ),
    );
}

/// The punched hole: the paper's lip, the desk through it, its shade.
fn punch(painter: &egui::Painter, t: &Tokens, rect: Rect) {
    ellipse(painter, rect, [50.0, 52.4, 31.5, 31.5, 0.0], t.paper, None);
    ellipse(painter, rect, [50.0, 50.0, 30.0, 30.0, 0.0], t.ground, None);
    let shade = Grad {
        cx: 0.5,
        cy: 0.4,
        r: 0.54,
        fx: 0.5,
        fy: 0.4,
        stops: &[
            (0.7, Color32::from_black_alpha(153)),
            (1.0, Color32::from_black_alpha(0)),
        ],
    };
    ellipse(
        painter,
        rect,
        [50.0, 50.0, 30.0, 30.0, 0.0],
        Color32::TRANSPARENT,
        Some(&shade),
    );
}
