//! Settings: this script's own formatting, saved beside it and rebuilt
//! into the book as the knobs move (was `desktop/ui/tune.js`, below its line).

use std::collections::VecDeque;

use egui::{Id, Rect, Sense, Ui, Vec2, accesskit};
use serde_json::{Value, json};

use super::widgets::{self, Look};
use super::{App, Purpose as AppPurpose, convert, read};
use crate::engine::{Reply, Request};
use crate::model::busy::Hold;
use crate::model::layout;
use crate::model::motion;
use crate::model::page_show::page_switches_from;
use crate::model::tune::{
    self as m, DefaultsOutcome, Kind, Knob, PaneMode, Phase, Preset, Settings, Status, can_explain,
    can_reset_defaults, coerce_value, current_preset, defaults_caption, defaults_write_args,
    defaults_write_outcome, display_value, empty_pane_mode, group_starts_open, groups, idle_reason,
    keep_choice_from, keep_write_args, keep_write_outcome, merge_pending, notes_for, option_keys,
    presets_from, restore_pending, same_settings, settings_from, status_for, switch_write_outcome,
    warning_lines,
};
use crate::theme::{self, size};

/// Long enough that a dragged slider is one save, short enough that a
/// single click does not feel ignored.
pub const SETTLE_MS: f64 = 300.0;

#[derive(Default)]
pub struct State {
    pub loaded: bool,
    pub settings: Option<Settings>,
    pub presets: Vec<Preset>,
    pub on_preset: Option<String>,
    pub app_defaults: Option<Settings>,
    pub shipped_defaults: Option<Settings>,
    pub defaults_busy: bool,
    pub defaults_note: String,
    pub keep_settings: Option<bool>,
    pub keep_shown: Option<bool>,
    pub keep_note: String,
    pub keep_seq: u64,
    keep_queue: VecDeque<(bool, u64, u64)>,
    keep_running: bool,
    pub switch_note: String,
    pub switch_seq: u64,
    switch_queue: VecDeque<(&'static str, bool, u64)>,
    switch_running: bool,
    switches_asked: bool,
    pub status: Status,
    pub fault: Option<Status>,
    pub warnings: Vec<String>,
    pub pending: Settings,
    pub deadline: Option<f64>,
    hold: Option<Hold>,
    pub era: u64,
    pub open: Vec<bool>,
}

#[derive(Clone, Debug)]
pub enum Purpose {
    Load {
        era: u64,
    },
    Save {
        era: u64,
        owed: Settings,
    },
    Rebuild {
        era: u64,
        owed: Settings,
    },
    Defaults {
        era: u64,
        confirmed: &'static str,
    },
    Keep {
        era: u64,
        seq: u64,
    },
    Switch {
        key: &'static str,
        on: bool,
        seq: u64,
    },
    SwitchesProbe,
}

fn tune_purpose(p: Purpose) -> AppPurpose {
    AppPurpose::Tune(p)
}

// ------------------------------------------------------------------ flows

pub fn script_changed(app: &mut App) {
    let s = &mut app.tune;
    s.era += 1;
    s.loaded = false;
    s.settings = None;
    s.presets.clear();
    s.on_preset = None;
    s.app_defaults = None;
    s.shipped_defaults = None;
    s.defaults_busy = false;
    s.defaults_note.clear();
    s.keep_settings = None;
    s.keep_shown = None;
    s.keep_note.clear();
    s.pending = Settings::new();
    s.warnings.clear();
    s.deadline = None;
    s.fault = None;
    s.status = Status::default();
    s.open = (0..groups().len()).map(group_starts_open).collect();
    release_hold(app);
}

pub fn shown(app: &mut App) {
    if app.tune.loaded || app.script.is_none() {
        return;
    }
    app.tune.loaded = true;
    load(app);
}

fn load(app: &mut App) {
    let Some(fountain) = app.script.as_ref().and_then(|s| s.fountain_path.clone()) else {
        app.tune.fault = Some(status_for(Phase::Failed(Some(m::FAULT_LINE))));
        return;
    };
    let era = app.tune.era;
    app.call(
        Request::Settings {
            fountain,
            set: None,
        },
        tune_purpose(Purpose::Load { era }),
    );
}

pub fn release_hold(app: &mut App) {
    if let Some(hold) = app.tune.hold.take() {
        let now = app.now();
        app.busy.release(hold, now);
    }
}

fn change(app: &mut App, knob: &Knob, raw: Value) {
    let Some(value) = coerce_value(knob, &raw) else {
        return;
    };
    let Some(settings) = app.tune.settings.as_mut() else {
        return;
    };
    if settings.get(knob.key) == Some(&value) {
        return;
    }
    app.tune.on_preset = None;
    settings.insert(knob.key.into(), value.clone());
    let settings = settings.clone();
    if let Some(script) = app.script.as_mut() {
        script.settings = Some(settings);
    }
    app.tune.pending = merge_pending(&app.tune.pending, knob.key, value);
    app.tune.status = status_for(Phase::Pending);
    schedule(app);
}

/// A preset overwrites every knob at once.
fn apply_all(app: &mut App, next: &Settings) {
    let Some(settings) = app.tune.settings.clone() else {
        return;
    };
    let mut changed = Settings::new();
    for key in option_keys() {
        if settings.get(key) != next.get(key)
            && let Some(v) = next.get(key)
        {
            changed.insert(key.into(), v.clone());
        }
    }
    if changed.is_empty() {
        return;
    }
    app.tune.on_preset = app
        .tune
        .presets
        .iter()
        .find(|p| {
            option_keys()
                .iter()
                .all(|k| p.settings.get(*k) == next.get(*k))
        })
        .map(|p| p.id.clone());
    let mut merged = settings;
    for (k, v) in &changed {
        merged.insert(k.clone(), v.clone());
    }
    app.tune.settings = Some(merged.clone());
    if let Some(script) = app.script.as_mut() {
        script.settings = Some(merged);
    }
    app.tune.pending = restore_pending(&app.tune.pending, &changed);
    app.tune.status = status_for(Phase::Pending);
    schedule(app);
}

fn schedule(app: &mut App) {
    app.tune.deadline = Some(app.now() + SETTLE_MS);
    if app.tune.hold.is_none() {
        app.tune.hold = Some(app.busy.hold());
    }
}

pub fn tick(app: &mut App, now: f64) {
    if app.tune.deadline.is_some_and(|d| now >= d) {
        settle(app);
    }
}

/// A settle still counting down is cut short whenever any page asks for a
/// turn on a book: its save goes first (book-queue.js beforeEveryTurn).
pub fn cut_settle_short(app: &mut App) {
    if app.tune.deadline.is_some() {
        settle(app);
    }
}

fn book_of(app: &App) -> String {
    app.script
        .as_ref()
        .and_then(|s| s.epub_path.clone().or_else(|| s.fountain_path.clone()))
        .unwrap_or_default()
}

/// The settle is over: what the moved knobs owe takes its turn on the book.
fn settle(app: &mut App) {
    app.tune.deadline = None;
    let owed = std::mem::take(&mut app.tune.pending);
    let fountain = app.script.as_ref().and_then(|s| s.fountain_path.clone());
    let (Some(fountain), false) = (fountain, owed.is_empty()) else {
        release_hold(app);
        return;
    };
    let book = book_of(app);
    let ahead = app.queue.holders(&book);
    if !ahead.is_empty() {
        app.tune.status = status_for(Phase::Waiting(&ahead));
    }
    let era = app.tune.era;
    let set = Value::Object(owed.clone()).to_string();
    // Taken directly rather than through on_book, which would cut this very
    // settle short again.
    app.save_on_book(
        &book,
        Request::Settings {
            fountain,
            set: Some(set),
        },
        tune_purpose(Purpose::Save { era, owed }),
    );
}

/// The save's engine call has started: the settle's hold can go.
pub fn save_started(app: &mut App) {
    app.tune.status = status_for(Phase::Saving);
    release_hold(app);
}

fn give_back(app: &mut App, owed: &Settings) {
    app.tune.pending = restore_pending(owed, &app.tune.pending);
}

pub fn answered(app: &mut App, purpose: Purpose, reply: Reply) {
    match purpose {
        Purpose::Load { era } => {
            if era != app.tune.era {
                return;
            }
            let answer = match reply.answer {
                Ok(a) => a,
                Err(message) => {
                    app.tune.loaded = false;
                    app.tune.fault = Some(status_for(Phase::Failed(Some(&message))));
                    return;
                }
            };
            let Some(settings) = settings_from(&answer) else {
                app.tune.loaded = false;
                app.tune.fault = Some(status_for(Phase::Failed(None)));
                return;
            };
            let s = &mut app.tune;
            s.fault = None;
            s.presets = presets_from(&answer);
            s.on_preset = current_preset(&answer);
            s.app_defaults = m::app_defaults_from(&answer);
            s.shipped_defaults = m::shipped_defaults_from(&answer);
            s.keep_settings = keep_choice_from(&answer);
            s.keep_shown = s.keep_settings;
            s.settings = Some(settings.clone());
            if let Some(script) = app.script.as_mut() {
                script.settings = Some(settings);
            }
        }
        Purpose::Save { era, owed } => {
            if era != app.tune.era {
                return;
            }
            let answer = match reply.answer {
                Ok(a) => a,
                Err(message) => {
                    give_back(app, &owed);
                    app.tune.status = status_for(Phase::Failed(Some(&message)));
                    return;
                }
            };
            let Some(saved) = settings_from(&answer) else {
                give_back(app, &owed);
                app.tune.status = status_for(Phase::Failed(None));
                return;
            };
            if let Some(script) = app.script.as_mut() {
                script.settings = Some(saved.clone());
            }
            if app.tune.pending.is_empty()
                && !same_settings(Some(&saved), app.tune.settings.as_ref())
            {
                app.tune.settings = Some(saved.clone());
            }
            let Some(script) = app.script.clone() else {
                return;
            };
            let (Some(fountain), Some(epub)) = (script.fountain_path, script.epub_path) else {
                app.tune.status = status_for(Phase::Saved);
                return;
            };
            app.continue_turn(
                Request::Reconvert {
                    fountain,
                    epub,
                    options: Value::Object(saved).to_string(),
                },
                tune_purpose(Purpose::Rebuild { era, owed }),
            );
        }
        Purpose::Rebuild { era, owed } => {
            if era != app.tune.era {
                return;
            }
            let answer = match reply.answer {
                Ok(a) => a,
                Err(message) => {
                    give_back(app, &owed);
                    app.tune.status = status_for(Phase::Failed(Some(&message)));
                    return;
                }
            };
            if answer.get("ok") != Some(&Value::Bool(true)) {
                give_back(app, &owed);
                let said = crate::model::json::error_message(&answer);
                app.tune.status = status_for(Phase::Failed(said.as_deref()));
                return;
            }
            if let Some(html) = answer.get("previewHtml").and_then(Value::as_str)
                && !html.trim().is_empty()
                && let Some(script) = app.script.as_mut()
            {
                script.preview_html = html.to_string();
            }
            read::rerendered(app);
            app.tune.status = status_for(if app.tune.pending.is_empty() {
                Phase::Saved
            } else {
                Phase::Pending
            });
            app.tune.warnings = warning_lines(&answer);
        }
        Purpose::Defaults { era, confirmed } => {
            let answer = App::answer_or(&reply);
            match defaults_write_outcome(era, app.tune.era, &answer, confirmed) {
                DefaultsOutcome::Stale => return,
                DefaultsOutcome::Applied {
                    message,
                    app: a,
                    shipped,
                } => {
                    app.tune.app_defaults = Some(a);
                    app.tune.shipped_defaults = Some(shipped);
                    app.tune.defaults_note = message;
                }
                DefaultsOutcome::Refused(message) => app.tune.defaults_note = message,
            }
            app.tune.defaults_busy = false;
        }
        Purpose::Keep { era, seq } => {
            let answer = App::answer_or(&reply);
            let outcome = keep_write_outcome(era, app.tune.era, seq, app.tune.keep_seq, &answer);
            if outcome.stored.is_some() {
                app.tune.keep_settings = outcome.stored;
            }
            if !outcome.stale {
                app.tune.keep_shown = app.tune.keep_settings;
                app.tune.keep_note = outcome.message;
            }
            app.tune.keep_running = false;
            next_keep(app);
        }
        Purpose::Switch { key, on, seq } => {
            app.tune.switch_running = false;
            if seq == app.tune.switch_seq {
                match switch_write_outcome(&App::answer_or(&reply)) {
                    Ok((animate, sounds)) => {
                        app.animate_page = Some(animate);
                        app.typewriter_sounds = Some(sounds);
                        app.tune.switch_note.clear();
                    }
                    Err(message) => {
                        set_switch(app, key, !on);
                        app.tune.switch_note = message;
                    }
                }
            }
            next_switch(app);
        }
        Purpose::SwitchesProbe => {
            if let Ok(answer) = reply.answer
                && let Some((animate, sounds)) = page_switches_from(&answer)
                && app.tune.switch_seq == 0
            {
                app.animate_page = Some(animate);
                app.typewriter_sounds = Some(sounds);
            }
        }
    }
}

fn write_defaults(app: &mut App, next: Option<Settings>, confirmed: &'static str) {
    app.tune.defaults_busy = true;
    app.tune.defaults_note.clear();
    let era = app.tune.era;
    app.call(
        Request::SetAppSettings {
            json: defaults_write_args(next.as_ref()),
        },
        tune_purpose(Purpose::Defaults { era, confirmed }),
    );
}

fn choose_keep(app: &mut App, keep: bool) {
    if app.tune.keep_shown == Some(keep) {
        return;
    }
    app.tune.keep_shown = Some(keep);
    app.tune.keep_note.clear();
    app.tune.keep_seq += 1;
    let (era, seq) = (app.tune.era, app.tune.keep_seq);
    app.tune.keep_queue.push_back((keep, era, seq));
    next_keep(app);
}

fn next_keep(app: &mut App) {
    if app.tune.keep_running {
        return;
    }
    while let Some((keep, era, seq)) = app.tune.keep_queue.pop_front() {
        if seq != app.tune.keep_seq {
            continue;
        }
        app.tune.keep_running = true;
        app.call(
            Request::SetAppSettings {
                json: keep_write_args(keep),
            },
            tune_purpose(Purpose::Keep { era, seq }),
        );
        return;
    }
}

fn set_switch(app: &mut App, key: &str, on: bool) {
    match key {
        "animatePage" => app.animate_page = Some(on),
        _ => app.typewriter_sounds = Some(on),
    }
}

fn flip_switch(app: &mut App, key: &'static str, on: bool) {
    set_switch(app, key, on);
    app.tune.switch_note.clear();
    app.tune.switch_seq += 1;
    let seq = app.tune.switch_seq;
    app.tune.switch_queue.push_back((key, on, seq));
    next_switch(app);
}

fn next_switch(app: &mut App) {
    if app.tune.switch_running {
        return;
    }
    if let Some((key, on, seq)) = app.tune.switch_queue.pop_front() {
        app.tune.switch_running = true;
        app.call(
            Request::SetAppSettings {
                json: json!({ key: on }).to_string(),
            },
            tune_purpose(Purpose::Switch { key, on, seq }),
        );
    }
}

fn send(app: &mut App) {
    convert::show_result(app);
    app.go_to(crate::model::frame::Surface::Convert);
    app.restore_focus = true;
}

// ---------------------------------------------------------------- drawing

fn head(app: &mut App, ui: &mut Ui, title: Option<&str>) {
    let t = app.tokens();
    ui.horizontal_wrapped(|ui| {
        match title {
            Some(title) => {
                widgets::title(ui, &t, title);
            }
            None => {
                widgets::caption(ui, &t, m::READING, false);
            }
        }
        ui.add_space(layout::SPACE[3]);
        let b = widgets::button(ui, &t, Look::Outline, crate::model::device::SEND_BOOK, true);
        app.stop(b.id, true);
        if b.clicked() {
            send(app);
        }
    });
}

pub fn draw(app: &mut App, ui: &mut Ui, block: layout::Span, window: Vec2) {
    let t = app.tokens();
    if app.script.is_none() {
        return;
    }
    if app.tune.open.is_empty() {
        app.tune.open = (0..groups().len()).map(group_starts_open).collect();
    }
    match empty_pane_mode(app.tune.settings.is_some(), app.tune.fault.as_ref()) {
        PaneMode::Reading => {
            head(app, ui, None);
            return;
        }
        PaneMode::Fault => {
            head(app, ui, Some(m::FAULT_SLUG));
            widgets::prose(ui, &t, m::FAULT_LINE);
            let line = app.tune.fault.clone().unwrap_or_default().line;
            widgets::caption(ui, &t, &line, true);
            ui.add_space(layout::SPACE[7]);
            let b = widgets::button(ui, &t, Look::Outline, m::FAULT_WAY, true);
            app.stop(b.id, true);
            if b.clicked() {
                convert::convert_again(app);
            }
            return;
        }
        PaneMode::Ready => {}
    }
    head(app, ui, Some(m::HEADING));
    ui.add_space(layout::SPACE[3]);
    widgets::prose(ui, &t, m::LEDE);
    let geometry = layout::settings(layout::Span {
        left: 0.0,
        width: block.width,
    });
    let top = ui.cursor().min;
    let knobs_rect = Rect::from_min_size(
        top + Vec2::new(geometry.knobs.left, 0.0),
        Vec2::new(geometry.knobs.width, f32::INFINITY),
    );
    let knobs = ui.scope_builder(egui::UiBuilder::new().max_rect(knobs_rect), |ui| {
        draw_knobs(app, ui)
    });
    let preview_top = if geometry.stacked {
        knobs.response.rect.bottom() + layout::SPACE[7]
    } else {
        top.y
    };
    let preview_h = if geometry.stacked {
        window.y * 0.44
    } else {
        (window.y - layout::SPACE[5] - layout::SPACE[7] - layout::SPACE[9]).max(260.0)
    };
    let preview_rect = Rect::from_min_size(
        egui::pos2(top.x + geometry.preview.left, preview_top),
        Vec2::new(geometry.preview.width, preview_h + 24.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(preview_rect), |ui| {
        widgets::state_label(ui, &t, "Preview");
        let frame =
            Rect::from_min_size(ui.cursor().min, Vec2::new(preview_rect.width(), preview_h));
        super::bookview::sheet(ui, &t, frame);
        let html = app
            .script
            .as_ref()
            .map(|s| s.preview_html.clone())
            .unwrap_or_default();
        if !html.trim().is_empty() {
            // The same document as Read, at this column's width; a swatch,
            // not somewhere you read: painted, hidden from the screen reader.
            let dark = app.dark;
            let inner = frame.shrink(1.0);
            let view = app
                .views
                .get(super::bookview::Place::Tune, &html, inner.width(), &t, dark);
            let content = view.height();
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("tune-preview")
                    .max_height(inner.height())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let (rect, _) = ui
                            .allocate_exact_size(Vec2::new(inner.width(), content), Sense::hover());
                        let visible = ui.clip_rect().intersect(rect);
                        if visible.height() > 0.0 {
                            view.paint_slice(
                                ui,
                                "tune-preview",
                                visible,
                                visible.top() - rect.top(),
                            );
                        }
                    });
            });
        }
        ui.allocate_rect(frame, Sense::hover());
    });
    let bottom = knobs.response.rect.bottom().max(preview_rect.bottom());
    ui.allocate_rect(
        Rect::from_min_max(top, egui::pos2(top.x + block.width, bottom)),
        Sense::hover(),
    );
}

fn draw_knobs(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    // Presets: the one the script is on is marked and inert.
    if !app.tune.presets.is_empty() {
        ui.add_space(layout::SPACE[6]);
        widgets::state_label(ui, &t, m::PRESET_LABEL);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = layout::SPACE[1];
            for preset in app.tune.presets.clone() {
                let on = !preset.id.is_empty() && Some(&preset.id) == app.tune.on_preset.as_ref();
                let b = widgets::button(ui, &t, Look::Small, &preset.display_name, !on);
                if on {
                    ui.ctx().accesskit_node_builder(b.id, |n| {
                        n.set_aria_current(accesskit::AriaCurrent::True)
                    });
                    ui.painter().rect_stroke(
                        b.rect,
                        theme::RADIUS,
                        egui::Stroke::new(1.0, t.brass),
                        egui::StrokeKind::Inside,
                    );
                }
                app.stop(b.id, !on);
                if b.clicked() {
                    apply_all(app, &preset.settings);
                }
            }
        });
        widgets::caption(ui, &t, m::PRESET_NOTE, false);
        ui.add_space(layout::SPACE[8]);
    }
    for (gi, group) in groups().into_iter().enumerate() {
        widgets::panel_frame(&t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let open = app.tune.open.get(gi).copied().unwrap_or(false);
            let title = group.title.to_string();
            let font = theme::chrome_strong(size::UI);
            // The disclosure triangle, then the title a space-2 after it.
            let triangle = ui.painter().layout_no_wrap(
                "\u{25B8}".into(),
                theme::chrome(size::LABEL),
                t.ink_muted,
            );
            let words = ui.painter().layout_no_wrap(title, font, t.ink);
            let gap = layout::SPACE[2];
            let size = Vec2::new(
                triangle.size().x + gap + words.size().x,
                triangle.size().y.max(words.size().y),
            );
            let (rect, r) = ui.allocate_exact_size(size, Sense::click());
            r.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, group.title)
            });
            ui.ctx()
                .accesskit_node_builder(r.id, |n| n.set_expanded(open));
            // It turns a quarter as the group opens, and the title's ink
            // follows the pointer, both over 0.2 s (surfaces.css
            // .knob-group-title).
            let turn = app.ease(
                r.id.with("turn"),
                if open { 1.0 } else { 0.0 },
                motion::GROUP_TURN,
            );
            let lit = app.ease(
                r.id.with("ink"),
                if r.hovered() { 1.0 } else { 0.0 },
                motion::STATE,
            );
            let half = triangle.size() / 2.0;
            let angle = turn * std::f32::consts::FRAC_PI_2;
            let at = rect.min + half - egui::emath::Rot2::from_angle(angle) * half;
            let at = at + Vec2::new(0.0, (size.y - triangle.size().y) / 2.0);
            ui.painter().add(
                egui::epaint::TextShape::new(at, triangle.clone(), t.ink_muted).with_angle(angle),
            );
            ui.painter().galley_with_override_text_color(
                rect.min + Vec2::new(triangle.size().x + gap, 0.0),
                words,
                super::bookview::mix(t.ink_soft, t.ink, lit),
            );
            widgets::focus_ring(ui, &t, &r, 3.0);
            app.stop(r.id, true);
            if r.clicked()
                && let Some(o) = app.tune.open.get_mut(gi)
            {
                *o = !*o;
            }
            if !open {
                return;
            }
            if let Some(note) = group.note {
                widgets::caption(ui, &t, note, false);
            }
            for knob in &group.knobs {
                draw_knob(app, ui, knob);
            }
        });
        ui.add_space(layout::SPACE[5]);
    }
    let status = app.tune.status.clone();
    widgets::live(
        ui,
        Id::new("tune-status"),
        &status.line,
        theme::chrome(size::LABEL),
        if status.bad { t.alarm } else { t.ink_muted },
    );
    let warnings = app.tune.warnings.join("\n");
    widgets::live(
        ui,
        Id::new("tune-warnings"),
        &warnings,
        theme::chrome(size::LABEL),
        t.ink_muted,
    );
    ui.add_space(layout::SPACE[6]);
    widgets::panel_frame(&t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        draw_defaults_foot(app, ui);
        draw_keep_choice(app, ui);
        draw_page_switches(app, ui);
    });
}

fn draw_knob(app: &mut App, ui: &mut Ui, knob: &Knob) {
    let t = app.tokens();
    let settings = app.tune.settings.clone();
    let idle = idle_reason(knob, settings.as_ref()).is_some();
    let value = settings.as_ref().and_then(|s| s.get(knob.key)).cloned();
    let label_ink = if idle { t.ink_muted } else { t.ink };
    let mut moved: Option<Value> = None;
    let mut control_id = None;
    ui.add_space(layout::SPACE[3]);
    ui.horizontal(|ui| {
        let label = widgets::text(ui, knob.label, theme::chrome(size::UI), label_ink);
        if knob.beta {
            widgets::beta_badge(ui, &t);
        }
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match &knob.kind {
                Kind::Toggle => {
                    let mut on = value.as_ref().and_then(Value::as_bool).unwrap_or(false);
                    let r = ui.add_enabled(!idle, egui::Checkbox::without_text(&mut on));
                    ui.ctx()
                        .accesskit_node_builder(r.id, |n| n.set_label(knob.label));
                    let _ = label.id;
                    control_id = Some(r.id);
                    if r.changed() {
                        moved = Some(Value::Bool(on));
                    }
                }
                Kind::Choice(choices) => {
                    let current = value
                        .as_ref()
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let shown = choices
                        .iter()
                        .find(|(c, _)| *c == current)
                        .map(|(_, l)| *l)
                        .unwrap_or("");
                    let combo = ui.add_enabled_ui(!idle, |ui| {
                        egui::ComboBox::from_id_salt(("knob", knob.key))
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                for (choice, words) in *choices {
                                    if ui.selectable_label(current == *choice, *words).clicked() {
                                        moved = Some(Value::String((*choice).into()));
                                    }
                                }
                            })
                    });
                    let r = combo.inner.response;
                    ui.ctx()
                        .accesskit_node_builder(r.id, |n| n.set_label(knob.label));
                    control_id = Some(r.id);
                }
                Kind::Range { .. } => {
                    widgets::text(
                        ui,
                        &display_value(knob, value.as_ref()),
                        theme::chrome(size::LABEL),
                        t.ink_muted,
                    );
                }
            },
        );
    });
    if let Kind::Range { min, max, step, .. } = knob.kind {
        let mut number = value.as_ref().and_then(Value::as_f64).unwrap_or(min);
        let r = ui.add_enabled(
            !idle,
            egui::Slider::new(&mut number, min..=max)
                .step_by(step)
                .show_value(false),
        );
        ui.ctx()
            .accesskit_node_builder(r.id, |n| n.set_label(knob.label));
        control_id = Some(r.id);
        if r.changed() {
            moved = Some(json!(number));
        }
    }
    if let Some(id) = control_id {
        app.stop(id, !idle);
    }
    if can_explain(knob) {
        let notes = notes_for(knob, settings.as_ref());
        if !notes.is_empty() {
            let why = widgets::caption(ui, &t, &notes.join(" "), false);
            if let Some(id) = control_id {
                widgets::described_by(ui, id, why.id);
            }
        }
    }
    let y = ui.cursor().min.y + layout::SPACE[1];
    ui.painter()
        .hline(ui.max_rect().x_range(), y, egui::Stroke::new(1.0, t.rule));
    ui.add_space(layout::SPACE[2]);
    if let Some(raw) = moved {
        change(app, knob, raw);
    }
}

fn draw_defaults_foot(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let (Some(a), Some(s)) = (
        app.tune.app_defaults.clone(),
        app.tune.shipped_defaults.clone(),
    ) else {
        return;
    };
    widgets::caption(ui, &t, defaults_caption(Some(&a), Some(&s)), false);
    let busy = app.tune.defaults_busy;
    ui.horizontal_wrapped(|ui| {
        let use_b = widgets::button(ui, &t, Look::Quiet, m::USE_DEFAULTS_LABEL, !busy);
        app.stop(use_b.id, !busy);
        if use_b.clicked() {
            let current = app.tune.settings.clone();
            write_defaults(app, current, m::USE_DEFAULTS_NOTE);
        }
        if can_reset_defaults(Some(&a), Some(&s)) {
            let reset = widgets::button(ui, &t, Look::Quiet, m::RESET_DEFAULTS_LABEL, !busy);
            app.stop(reset.id, !busy);
            if reset.clicked() {
                write_defaults(app, None, m::RESET_DEFAULTS_NOTE);
            }
        }
    });
    let note = app.tune.defaults_note.clone();
    widgets::live(
        ui,
        Id::new("defaults-note"),
        &note,
        theme::chrome(size::LABEL),
        t.ink_muted,
    );
}

fn draw_keep_choice(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    if app.tune.keep_settings.is_none() {
        return;
    }
    ui.add_space(layout::SPACE[6]);
    let group = ui.scope(|ui| {
        widgets::state_label(ui, &t, m::KEEP_LEGEND);
        for option in &m::KEEP_OPTIONS {
            let checked = app.tune.keep_shown == Some(option.keep);
            let r = ui.radio(
                checked,
                egui::RichText::new(option.label)
                    .font(theme::chrome(size::UI))
                    .color(t.ink),
            );
            app.stop(r.id, true);
            let line = widgets::caption(ui, &t, option.line, false);
            widgets::described_by(ui, r.id, line.id);
            if r.clicked() {
                choose_keep(app, option.keep);
            }
        }
        let note = app.tune.keep_note.clone();
        widgets::live(
            ui,
            Id::new("keep-note"),
            &note,
            theme::chrome(size::LABEL),
            t.ink_muted,
        );
    });
    ui.ctx().accesskit_node_builder(group.response.id, |n| {
        n.set_role(accesskit::Role::RadioGroup);
        n.set_bounds(widgets::bounds(group.response.rect));
        n.set_label(m::KEEP_LEGEND);
    });
}

fn draw_page_switches(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    if page_switches_from(
        &json!({"animatePage": app.animate_page, "typewriterSounds": app.typewriter_sounds}),
    )
    .is_none()
        && !app.tune.switches_asked
    {
        app.tune.switches_asked = true;
        app.call(Request::AppSettings, tune_purpose(Purpose::SwitchesProbe));
    }
    ui.add_space(layout::SPACE[6]);
    let group = ui.scope(|ui| {
        widgets::state_label(ui, &t, m::SWITCHES_LEGEND);
        let animate_on = app.animate_page != Some(false);
        for option in &m::SWITCH_OPTIONS {
            let mut on = if option.key == "animatePage" {
                animate_on
            } else {
                app.typewriter_sounds == Some(true)
            };
            let enabled = option.key == "animatePage" || animate_on;
            let r = ui.add_enabled(
                enabled,
                egui::Checkbox::new(
                    &mut on,
                    egui::RichText::new(option.label)
                        .font(theme::chrome(size::UI))
                        .color(t.ink),
                ),
            );
            app.stop(r.id, enabled);
            let line = widgets::caption(ui, &t, option.line, false);
            widgets::described_by(ui, r.id, line.id);
            if r.changed() {
                flip_switch(app, option.key, on);
            }
        }
        if app.reduced_motion() {
            widgets::caption(ui, &t, m::SWITCHES_REDUCED, false);
        }
        let note = app.tune.switch_note.clone();
        widgets::live(
            ui,
            Id::new("switch-note"),
            &note,
            theme::chrome(size::LABEL),
            t.ink_muted,
        );
    });
    ui.ctx().accesskit_node_builder(group.response.id, |n| {
        n.set_role(accesskit::Role::Group);
        n.set_bounds(widgets::bounds(group.response.rect));
        n.set_label(m::SWITCHES_LEGEND);
    });
}
