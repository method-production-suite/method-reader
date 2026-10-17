//! Read: the converted script, keeping its shape, with its scene index
//! (was `desktop/ui/read.js`, below its line).

use egui::{Id, Key, Pos2, Rect, Sense, Ui, Vec2, WidgetInfo, WidgetType, accesskit};

use super::bookview::{Place as Shown, sheet};
use super::widgets::{self, Look};
use super::{App, convert};
use crate::model::device::SEND_BOOK;
use crate::model::layout;
use crate::model::motion::{self, Timing};
use crate::model::read::{
    self as m, Mark, Place, RailKey, ReadKey, ReaderState, Scroll, index_toggle, rail_count,
    rail_entries, rail_step, reader_place, reader_state, scroll_step, scroll_target,
};
use crate::theme::{self, size};

pub struct State {
    pub index_open: bool,
    pub place: Option<Place>,
    pub offset: f32,
    pub marks: Vec<Mark>,
    pub rail_stop: usize,
    pub marked: Option<String>,
    /// A scroll the next frame owes (a place to restore, a key pressed).
    pub scroll_to: Option<f32>,
    /// A scene pressed in the index, gliding there (R9): from, to, when it
    /// began, and how (WebKit's smooth scroll).
    pub glide: Option<(f32, f32, f64, Timing)>,
    pub restore: bool,
    pub frame_height: f32,
    pub content_height: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            index_open: true,
            place: None,
            offset: 0.0,
            marks: Vec::new(),
            rail_stop: 0,
            marked: None,
            scroll_to: None,
            glide: None,
            restore: false,
            frame_height: 0.0,
            content_height: 0.0,
        }
    }
}

pub fn script_changed(app: &mut App) {
    app.read = State::default();
}

/// Coming back: put the reader where they were.
pub fn shown(app: &mut App) {
    app.read.restore = app.read.place.is_some();
}

/// Leaving: take the place while the marks still describe the page.
pub fn hidden(app: &mut App) {
    keep(app);
}

fn keep(app: &mut App) {
    if !app.read.marks.is_empty()
        && let Some(place) = reader_place(&app.read.marks, app.read.offset)
    {
        app.read.place = Some(place);
    }
}

/// The book was rebuilt (Settings saved): keep the place across the reflow.
pub fn rerendered(app: &mut App) {
    keep(app);
    app.read.restore = app.read.place.is_some();
}

fn send(app: &mut App) {
    convert::show_result(app);
    if app.surface == crate::model::frame::Surface::Convert {
        app.restore_focus = true;
    } else {
        app.go_to(crate::model::frame::Surface::Convert);
    }
}

/// `end` is where the surface must stop (a screen y) for the page's foot,
/// the rev stamp, to sit whole inside the window: Read fits the window.
pub fn draw(app: &mut App, ui: &mut Ui, block: layout::Span, window: Vec2, end: f32) {
    let t = app.tokens();
    match reader_state(app.script.as_ref()) {
        ReaderState::Closed => {}
        ReaderState::Blank => {
            widgets::title(ui, &t, m::BLANK_SLUG);
            ui.add_space(layout::SPACE[3]);
            widgets::prose(ui, &t, m::BLANK_LINE);
            ui.add_space(layout::SPACE[7]);
            ui.horizontal(|ui| {
                let again = widgets::button(ui, &t, Look::Outline, m::BLANK_WAY, true);
                let send_b = widgets::button(ui, &t, Look::Outline, SEND_BOOK, true);
                app.stop(again.id, true);
                app.stop(send_b.id, true);
                if again.clicked() {
                    convert::convert_again(app);
                }
                if send_b.clicked() {
                    send(app);
                }
            });
        }
        ReaderState::Ready => draw_ready(app, ui, block, window, end),
    }
}

fn draw_ready(app: &mut App, ui: &mut Ui, block: layout::Span, window: Vec2, end: f32) {
    let t = app.tokens();
    let Some(script) = app.script.clone() else {
        return;
    };
    let html = script.preview_html.clone();
    // The head: the title, the author, the index toggle, Send.
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            widgets::heading(
                ui,
                &script.title,
                theme::chrome_strong(size::CAPTION),
                t.ink,
            );
            if let Some(author) = &script.author {
                widgets::text(
                    ui,
                    &format!("by {author}"),
                    theme::chrome(size::LABEL),
                    t.ink_muted,
                );
            }
        });
        ui.add_space(layout::SPACE[5]);
        let (label, expanded) = index_toggle(app.read.index_open);
        let toggle = widgets::button(ui, &t, Look::Quiet, label, true);
        ui.ctx()
            .accesskit_node_builder(toggle.id, |n| n.set_expanded(expanded));
        app.stop(toggle.id, true);
        if toggle.clicked() {
            app.read.index_open = !app.read.index_open;
        }
        ui.add_space(layout::SPACE[5]);
        let send_b = widgets::button(ui, &t, Look::Quiet, SEND_BOOK, true);
        app.stop(send_b.id, true);
        if send_b.clicked() {
            send(app);
        }
    });
    let rule_y = ui.cursor().min.y + layout::SPACE[3];
    ui.painter().hline(
        ui.max_rect().x_range(),
        rule_y,
        egui::Stroke::new(1.0, t.rule),
    );
    ui.add_space(layout::SPACE[3] + layout::SPACE[5]);

    let geometry = layout::read(layout::Span {
        left: 0.0,
        width: block.width,
    });
    let top = ui.cursor().min;
    // The frame's height for a frame whose top is at `y`: the window's
    // share, giving way so the foot stays inside the window.
    let frame_height = |y: f32| layout::script_frame(window.x, window.y, end - y);
    // The book, laid out at the script column's width: its scenes feed the
    // index, its slices are what is read.
    let book_width = if geometry.stacked {
        block.width
    } else {
        geometry.script.width
    } - 2.0;
    let dark = app.dark;
    // Until the book's first layout comes from its thread there is no
    // index to draw (not an empty one: that would say there are no scenes).
    let scenes = app
        .views
        .get(Shown::Read, &html, book_width, &t, dark)
        .laid()
        .map(|l| l.scenes.clone());
    let laid = scenes.is_some();
    let entries = rail_entries(&scenes.unwrap_or_default());
    let mut rail_bottom = top.y;
    let open = app.read.index_open;
    // Beside the script, opening and shutting slides the index 0.8rem and
    // fades it over 0.2 s, in its own slot, so the script never moves
    // (surfaces.css .scene-rail); stacked above it, it is there or absent.
    let shown = if geometry.stacked {
        if open { 1.0 } else { 0.0 }
    } else {
        app.ease(
            Id::new("scene-rail-shown"),
            if open { 1.0 } else { 0.0 },
            motion::INDEX_SLIDE,
        )
    };
    if shown > 0.0 && laid {
        let rail_rect = if geometry.stacked {
            Rect::from_min_size(top, Vec2::new(block.width, 104.0))
        } else {
            Rect::from_min_size(
                top + Vec2::new(geometry.index.left - (1.0 - shown) * layout::SPACE[3], 0.0),
                Vec2::new(geometry.index.width, frame_height(top.y)),
            )
        };
        let r = ui.scope_builder(egui::UiBuilder::new().max_rect(rail_rect), |ui| {
            ui.multiply_opacity(shown);
            draw_rail(app, ui, &entries, geometry.stacked, open)
        });
        // Stacked, the script starts a gap below the index; beside it, the
        // index ends with the frame and adds no gap under the page.
        rail_bottom = r.response.rect.bottom()
            + if geometry.stacked {
                layout::SPACE[4]
            } else {
                0.0
            };
    }
    let script_top = if geometry.stacked { rail_bottom } else { top.y };
    let height = frame_height(script_top);
    let script_rect = Rect::from_min_size(
        Pos2::new(top.x + geometry.script.left, script_top),
        Vec2::new(geometry.script.width, height),
    );
    // The stage: the reader's Tab stop, a group named for the script.
    let stage_id = Id::new("script-stage");
    let stage = ui.interact(script_rect, stage_id, Sense::focusable_noninteractive());
    ui.ctx().accesskit_node_builder(stage.id, |n| {
        n.set_role(accesskit::Role::Group);
        n.set_label(format!("{}, the script", script.title));
    });
    app.stop(stage.id, true);
    sheet(ui, &t, script_rect);
    widgets::own_arrows(ui, stage.id);
    if widgets::ring_shown(ui, &stage) {
        ui.painter().rect_stroke(
            script_rect.expand(2.0),
            theme::RADIUS_WELL,
            egui::Stroke::new(2.0, t.ink),
            egui::StrokeKind::Outside,
        );
    }
    if stage.has_focus() {
        let key = ui.input(|i| {
            [
                (Key::ArrowDown, ReadKey::Down),
                (Key::ArrowUp, ReadKey::Up),
                (Key::PageDown, ReadKey::PageDown),
                (Key::PageUp, ReadKey::PageUp),
                (Key::Space, ReadKey::Space),
                (Key::Home, ReadKey::Home),
                (Key::End, ReadKey::End),
            ]
            .into_iter()
            .find(|(k, _)| i.key_pressed(*k))
            .map(|(_, r)| r)
        });
        if let Some(key) = key {
            let max = (app.read.content_height - height).max(0.0);
            app.read.glide = None;
            app.read.scroll_to = Some(match scroll_step(key, height) {
                Scroll::By(d) => (app.read.offset + d).clamp(0.0, max),
                Scroll::Top => 0.0,
                Scroll::Bottom => max,
            });
        }
    }
    let now = app.now();
    let view = app.views.get(Shown::Read, &html, book_width, &t, dark);
    let marks = view.marks();
    let content = view.height();
    let fresh = view.fresh();
    app.read.marks = marks.clone();
    app.read.content_height = content;
    app.read.frame_height = height;
    // The place goes back once the layout it is measured in is the one
    // shown (a rebuilt book's, not the one before it).
    if app.read.restore && fresh {
        app.read.restore = false;
        app.read.scroll_to = Some(scroll_target(&marks, app.read.place.as_ref()));
    }
    let mut area = egui::ScrollArea::vertical()
        .id_salt("script-frame")
        .max_height(height)
        .auto_shrink([false, false]);
    let mut gliding_at = None;
    let mut gliding = false;
    if let Some(to) = app.read.scroll_to.take() {
        app.read.glide = None;
        area = area.vertical_scroll_offset(to);
    } else if let Some((from, to, start, timing)) = app.read.glide {
        let elapsed = now - start;
        let y = from + (to - from) * timing.progress(elapsed);
        area = area.vertical_scroll_offset(y);
        gliding_at = Some(y);
        gliding = timing.running(elapsed);
        if !gliding {
            app.read.glide = None;
        }
    }
    let out = ui.scope_builder(
        egui::UiBuilder::new().max_rect(script_rect.shrink(1.0)),
        |ui| {
            area.show(ui, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(book_width, content), Sense::hover());
                // Only the slice on screen is painted.
                let visible = ui.clip_rect().intersect(rect);
                if visible.height() > 0.0 {
                    view.paint_slice(ui, "read", visible, visible.top() - rect.top());
                }
                // The script's words, block by block, for the screen reader.
                view.speak(ui, "read", rect.min, visible);
            })
        },
    );
    app.read.offset = out.inner.state.offset.y;
    // The reader scrolled during a glide: theirs wins, as in a browser.
    if gliding_at.is_some_and(|y| (y - app.read.offset).abs() > 0.5) {
        app.read.glide = None;
    } else if gliding {
        app.moving();
    }
    mark_current(app);
    let bottom = script_rect.bottom().max(rail_bottom);
    ui.allocate_rect(
        Rect::from_min_max(top, Pos2::new(top.x + block.width, bottom)),
        Sense::hover(),
    );
}

/// An entry in the scene index pressed: its scene to the top (R9),
/// smoothly as WebKit scrolls one into view, at once with Reduce Motion.
pub fn go_to_scene(app: &mut App, id: &str) {
    let Some(top) = app.read.marks.iter().find(|m| m.id == id).map(|m| m.top) else {
        return;
    };
    let max = (app.read.content_height - app.read.frame_height).max(0.0);
    let to = top.min(max);
    let from = app.read.offset;
    if app.reduced_motion() || (to - from).abs() < 0.5 {
        app.read.glide = None;
        app.read.scroll_to = Some(to);
        return;
    }
    app.read.glide = Some((from, to, app.now(), motion::scroll_timing(to - from)));
}

/// Mark the scene being read; the index's stop follows it unless the
/// keyboard is walking the index.
fn mark_current(app: &mut App) {
    let Some(at) = reader_place(&app.read.marks, app.read.offset) else {
        return;
    };
    if app.read.marked.as_deref() == Some(at.id.as_str()) {
        return;
    }
    app.read.marked = Some(at.id.clone());
    if let Some(i) = app.read.marks.iter().position(|m| m.id == at.id) {
        app.read.rail_stop = i;
    }
}

/// `open` is false while it fades away: still drawn, out of the Tab order.
fn draw_rail(app: &mut App, ui: &mut Ui, entries: &[m::RailEntry], stacked: bool, open: bool) {
    let t = app.tokens();
    let nav = ui.scope(|ui| {
        if entries.is_empty() {
            widgets::caption(ui, &t, m::NO_SCENES, false);
            return;
        }
        widgets::text(
            ui,
            &rail_count(entries.len()),
            theme::chrome_strong(size::LABEL),
            t.ink_muted,
        );
        let mut walk: Option<usize> = None;
        let mut area = egui::ScrollArea::vertical()
            .id_salt("scene-rail")
            .auto_shrink([false, false]);
        if stacked {
            area = area.max_height(104.0);
        }
        area.show(ui, |ui| {
            let layout = if stacked {
                egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true)
            } else {
                egui::Layout::top_down(egui::Align::Min)
            };
            ui.with_layout(layout, |ui| {
                for (i, entry) in entries.iter().enumerate() {
                    let current = app.read.marked.as_deref() == Some(entry.id.as_str());
                    let stop = i == app.read.rail_stop;
                    let id = Id::new(("scene-link", &entry.id));
                    let mut job = egui::text::LayoutJob::default();
                    job.wrap.max_width = ui.available_width() - 8.0;
                    let ink = if current { t.ink } else { t.ink_muted };
                    // A scene's heading is the script's own words: its face
                    // and its capitals, as the page sets them.
                    job.append(
                        &entry.label.place.to_uppercase(),
                        0.0,
                        egui::TextFormat {
                            font_id: theme::structure(size::NOTE),
                            color: ink,
                            ..Default::default()
                        },
                    );
                    if let Some(time) = &entry.label.time {
                        job.append(
                            &format!(" {} {}", m::SEPARATOR, time.to_uppercase()),
                            0.0,
                            egui::TextFormat {
                                font_id: theme::structure(size::NOTE),
                                color: ink.gamma_multiply(0.62),
                                ..Default::default()
                            },
                        );
                    }
                    let galley = ui.fonts_mut(|f| f.layout_job(job));
                    let (rect, _) = ui
                        .allocate_exact_size(galley.size() + Vec2::new(12.8, 6.0), Sense::hover());
                    let sense = if stop { Sense::click() } else { Sense::CLICK };
                    let r = ui.interact(rect, id, sense);
                    let name = match &entry.label.time {
                        Some(time) => format!("{} {} {}", entry.label.place, m::SEPARATOR, time),
                        None => entry.label.place.clone(),
                    };
                    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &name));
                    if current {
                        ui.ctx().accesskit_node_builder(r.id, |n| {
                            n.set_aria_current(accesskit::AriaCurrent::True)
                        });
                        ui.painter().rect_filled(rect, theme::RADIUS, t.brass_soft);
                    }
                    ui.painter()
                        .galley(rect.min + Vec2::new(6.4, 3.0), galley, ink);
                    widgets::focus_ring(ui, &t, &r, 1.0);
                    if stop && open {
                        app.stop(r.id, true);
                    }
                    if r.clicked() {
                        go_to_scene(app, &entry.id);
                    }
                    widgets::own_arrows(ui, r.id);
                    if r.has_focus() {
                        let key = ui.input(|i| {
                            [
                                (Key::ArrowDown, RailKey::Down),
                                (Key::ArrowUp, RailKey::Up),
                                (Key::Home, RailKey::Home),
                                (Key::End, RailKey::End),
                            ]
                            .into_iter()
                            .find(|(k, _)| i.key_pressed(*k))
                            .map(|(_, r)| r)
                        });
                        if let Some(key) = key {
                            walk = rail_step(key, i, entries.len());
                        }
                    }
                }
            });
        });
        if let Some(next) = walk {
            app.read.rail_stop = next;
            ui.memory_mut(|m| m.request_focus(Id::new(("scene-link", &entries[next].id))));
        }
    });
    ui.ctx().accesskit_node_builder(nav.response.id, |n| {
        n.set_role(accesskit::Role::Navigation);
        n.set_bounds(widgets::bounds(nav.response.rect));
        n.set_label("Scenes");
    });
}
