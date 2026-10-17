//! Convert: the drop well, the bar, the result and the refusal
//! (was `desktop/ui/convert.js`, below its line). Every rule is the model's.

use std::collections::HashMap;

use egui::{Color32, Id, Rect, Sense, Shape, Stroke, Ui, Vec2, accesskit};
use serde_json::Value;

use super::bookview::{BOOK_EM, Place, sheet};
use super::widgets::{self, Look};
use super::{App, Purpose as AppPurpose};
use crate::engine::{Reply, Request};
use crate::model::convert::{
    self as m, Library, LibraryAction, Progress, Refusal, failure_for, internal_failure,
};
use crate::model::feedback::{new_issue_url, os_label};
use crate::model::frame::Surface;
use crate::model::layout;
use crate::model::notes::RELEASE;
use crate::model::page_show::{can_show, page_paragraphs, page_switches_from};
use crate::theme::{self, size};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Idle,
    Working,
    Done,
    Failed,
}

#[derive(Default)]
pub struct State {
    pub view: View,
    pub converting: bool,
    /// The library book each PDF converted in this window landed as.
    pub landed: HashMap<String, String>,
    pub generation: u64,
    pub library: Option<Library>,
    pub secondary: String,
    pub library_busy: bool,
    pub known_home: Option<String>,
    pub path: String,
    pub progress: Progress,
    pub waiting: Option<String>,
    pub book: String,
    pub reveal_note: Option<(String, String)>,
    pub refusal: Option<Refusal>,
    pub failed_path: String,
}

#[derive(Clone, Debug)]
pub enum Purpose {
    LibraryProbe {
        generation: u64,
    },
    LibraryWrite {
        action: LibraryAction,
        previous: String,
    },
    Convert {
        path: String,
    },
    Reveal {
        book: String,
    },
}

// ------------------------------------------------------------------ flows

/// Back to the empty state, with its library probe.
pub fn draw_well(app: &mut App) {
    let c = &mut app.convert;
    c.view = View::Idle;
    c.generation += 1;
    c.library = None;
    c.secondary.clear();
    c.library_busy = false;
    c.refusal = None;
    let generation = c.generation;
    app.call(
        Request::AppSettings,
        AppPurpose::Convert(Purpose::LibraryProbe { generation }),
    );
}

/// Ask the OS for a screenplay (the well's button and Cmd/Ctrl-O).
pub fn choose(app: &mut App) {
    if app.convert.converting || !app.dialogs.try_open(&mut app.busy) {
        return;
    }
    let picked = app.host.dialogs.pick_screenplay();
    let now = app.now();
    app.dialogs.closed(&mut app.busy, now);
    app.restore_focus = true;
    if let Some(path) = picked {
        convert_path(app, &path, false);
    }
}

/// A file arrived from the desktop.
pub fn drop_paths(app: &mut App, paths: &[String]) {
    if let Some(path) = m::dropped_path(paths) {
        convert_path(app, &path, false);
    }
}

pub fn convert_path(app: &mut App, path: &str, force: bool) {
    if app.convert.converting {
        return;
    }
    app.convert.converting = true;
    super::show::stop(app);
    app.go_to(Surface::Convert);
    let book = app.convert.landed.get(path).cloned();
    let ahead = book
        .as_deref()
        .map(|b| app.queue.holders(b))
        .unwrap_or_default();
    super::panel::hidden(app);
    let c = &mut app.convert;
    c.view = View::Working;
    c.path = path.to_string();
    c.progress = Progress::start();
    c.waiting = m::waiting_label(&ahead);
    app.restore_focus = true;
    let request = Request::Convert {
        path: path.into(),
        force,
        options: None,
    };
    let purpose = AppPurpose::Convert(Purpose::Convert { path: path.into() });
    match book {
        Some(book) => app.on_book(&book, "convert", request, purpose),
        None => app.call(request, purpose),
    }
}

/// The conversion's turn has come: the read-out stops naming the wait.
pub fn started(app: &mut App) {
    app.convert.waiting = None;
    // A fresh bar for a fresh conversion, as drawProgress built one.
    app.forget_motion(Id::new(FILL));
}

/// The progress bar's fill, easing to each new figure (surfaces.css .fill).
const FILL: &str = "progress-fill";

/// One progress line from the engine.
pub fn progress(app: &mut App, purpose: &Purpose, line: &Value) {
    let Purpose::Convert { path } = purpose else {
        return;
    };
    if app.convert.waiting.is_some() {
        started(app);
    }
    if page_paragraphs(line).is_some() {
        if app.show.is_none() && can_show(app.reduced_motion(), app.animate_page, line) {
            let paragraphs = page_paragraphs(line).unwrap_or_default();
            let sounds = app.typewriter_sounds == Some(true);
            let now = app.now();
            app.show = Some(super::show::Show::begin(
                &paragraphs,
                m::file_name(path),
                now,
                sounds,
            ));
            app.restore_focus = true;
        }
        return;
    }
    if let Some(show) = app.show.as_mut() {
        show.progress(line);
    }
    if let Some(next) = m::next_progress(&app.convert.progress, line) {
        app.convert.progress = next;
    }
}

pub fn answered(app: &mut App, purpose: Purpose, reply: Reply) {
    match purpose {
        Purpose::LibraryProbe { generation } => {
            let Ok(answer) = reply.answer else { return };
            if let Some((animate, sounds)) = page_switches_from(&answer) {
                app.animate_page = Some(animate);
                app.typewriter_sounds = Some(sounds);
            }
            if generation != app.convert.generation || app.convert.view != View::Idle {
                return;
            }
            if let Some(library) = m::library_from(&answer) {
                app.convert.known_home = Some(library.home.clone());
                app.convert.library = Some(library);
            }
        }
        Purpose::LibraryWrite { action, previous } => {
            let c = &mut app.convert;
            c.library_busy = false;
            match reply.answer {
                Err(message) => c.secondary = message,
                Ok(answer) => match m::library_after(&answer) {
                    Ok(library) => {
                        c.secondary = if m::shows_moved_line(action, &previous, &library) {
                            m::MOVED_LINE.into()
                        } else {
                            String::new()
                        };
                        c.library = Some(library);
                    }
                    Err(message) => c.secondary = message,
                },
            }
        }
        Purpose::Convert { path } => {
            app.convert.converting = false;
            match reply.answer {
                Err(message) => {
                    super::show::stop(app);
                    draw_failure(app, internal_failure(&message), &path);
                }
                Ok(answer) if crate::model::json::ok(&answer) => {
                    if let Some(epub) = answer.get("epubPath").and_then(Value::as_str) {
                        app.convert.landed.insert(path.clone(), epub.into());
                    }
                    draw_result(app, &path, &answer);
                }
                Ok(answer) => {
                    super::show::stop(app);
                    draw_failure(app, failure_for(answer.get("error")), &path);
                }
            }
        }
        Purpose::Reveal { book } => {
            let note = match reply.answer {
                Ok(answer) => m::reveal_failure_message(&answer).unwrap_or_default(),
                Err(message) => message,
            };
            app.convert.reveal_note = Some((book, note));
        }
    }
}

fn draw_result(app: &mut App, path: &str, answer: &Value) {
    let script = m::script_from(path, answer);
    if let Some(result) = &app.send_result
        && result.book != script.epub_path
        && result.eject.is_none()
    {
        app.send_result = None;
    }
    app.script = Some(script);
    paint_result(app);
    app.script_changed();
    // The page show learns of the book once its first page is laid out
    // (adopt_when_laid).
    app.restore_focus = true;
}

/// The result: the view, its book, and the send panel mounted for it.
fn paint_result(app: &mut App) {
    let book = app
        .script
        .as_ref()
        .and_then(|s| s.epub_path.clone())
        .unwrap_or_default();
    app.convert.view = View::Done;
    app.convert.book = book;
    app.convert.refusal = None;
    super::panel::mount(app);
    if app.surface == Surface::Convert {
        super::panel::shown(app);
    }
}

fn draw_failure(app: &mut App, refusal: Refusal, path: &str) {
    super::panel::hidden(app);
    app.convert.view = View::Failed;
    app.convert.refusal = Some(refusal);
    app.convert.failed_path = path.to_string();
    app.restore_focus = true;
}

/// "Convert another": the well, the book still open behind it.
pub fn reset(app: &mut App) {
    if app.convert.converting {
        return;
    }
    super::show::stop(app);
    super::panel::hidden(app);
    draw_well(app);
}

/// Read's and Settings' Send: the open book's result, drawn again only if
/// the well or a refusal replaced it.
pub fn show_result(app: &mut App) {
    let Some(script) = &app.script else { return };
    if app.convert.converting {
        return;
    }
    let book = script.epub_path.clone().unwrap_or_default();
    if app.convert.view == View::Done && app.convert.book == book {
        return;
    }
    paint_result(app);
}

/// "Convert it again" wherever it is offered.
pub fn convert_again(app: &mut App) {
    let path = app
        .script
        .as_ref()
        .map(|s| s.path.clone())
        .unwrap_or_default();
    if app.convert.converting || path.trim().is_empty() {
        app.go_to(Surface::Convert);
        return;
    }
    convert_path(app, &path, false);
}

fn library_run(app: &mut App, action: LibraryAction) {
    let Some(library) = app.convert.library.clone() else {
        return;
    };
    let mut path = None;
    if action == LibraryAction::Change {
        if !app.dialogs.try_open(&mut app.busy) {
            return;
        }
        app.convert.library_busy = true;
        let picked = app.host.dialogs.pick_folder(&library.path);
        let now = app.now();
        app.dialogs.closed(&mut app.busy, now);
        app.restore_focus = true;
        app.convert.library_busy = false;
        match picked {
            Some(p) if p != library.path => path = Some(p),
            _ => return,
        }
    }
    app.convert.library_busy = true;
    app.call(
        Request::SetAppSettings {
            json: m::library_change_args(path.as_deref()),
        },
        AppPurpose::Convert(Purpose::LibraryWrite {
            action,
            previous: library.path,
        }),
    );
}

fn reveal(app: &mut App, book: String) {
    app.call(
        Request::Reveal { path: book.clone() },
        AppPurpose::Convert(Purpose::Reveal { book }),
    );
}

fn report(app: &mut App, refusal: &Refusal) {
    let url = new_issue_url(
        RELEASE.version,
        &os_label(app.host.platform.word()),
        Some(&format!("{}: {}", refusal.code, refusal.message)),
        app.convert.known_home.as_deref(),
    );
    app.host.open(&url);
}

// ---------------------------------------------------------------- drawing

pub fn draw(app: &mut App, ui: &mut Ui, block: layout::Span, window: Vec2) {
    match app.convert.view {
        View::Idle => column(ui, |ui| draw_idle(app, ui, window)),
        View::Working if app.show.is_some() => super::show::draw(app, ui, block),
        View::Working => column(ui, |ui| draw_working(app, ui)),
        View::Done => draw_done(app, ui, block),
        View::Failed => column(ui, |ui| draw_failed(app, ui)),
    }
}

/// Convert keeps its own 820 column in every state but the result.
fn column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let width = ui.available_width().min(layout::CONVERT_COLUMN);
    let pad = (ui.available_width() - width) / 2.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(width);
            add(ui);
        });
    });
}

fn draw_idle(app: &mut App, ui: &mut Ui, window: Vec2) {
    let t = app.tokens();
    // The app's name is the title bar's; the page is the drop well alone,
    // set a calm distance down (less in a short window).
    let short = window.y <= layout::SHORT_PAGE;
    ui.add_space(if short {
        layout::SPACE[5]
    } else {
        layout::SPACE[10]
    });
    let targeted = app.drag_hover;
    let frame = egui::Frame::NONE
        .fill(if targeted { t.brass_soft } else { t.panel })
        .corner_radius(theme::RADIUS_WELL)
        .inner_margin(egui::Margin::symmetric(24, 36));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = layout::SPACE[2];
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
            paint_well_mark(ui, rect, if targeted { t.brass } else { t.ink_muted });
            ui.add_space(layout::SPACE[1]);
            let call = widgets::text(ui, m::WELL_CALL, theme::chrome_strong(size::BODY), t.ink);
            ui.ctx().accesskit_node_builder(call.id, |n| {
                n.set_role(accesskit::Role::Heading);
                n.set_level(1);
                n.set_label(m::WELL_CALL);
            });
            widgets::text(ui, m::WELL_OR, theme::chrome(size::LABEL), t.ink_muted);
            let b = widgets::button(ui, &t, Look::Brass, m::CHOOSE, true);
            let shortcut = m::shortcut_label(app.host.platform);
            ui.ctx()
                .accesskit_node_builder(b.id, |n| n.set_keyboard_shortcut(shortcut));
            app.stop(b.id, true);
            if b.clicked() {
                choose(app);
            }
            ui.add_space(layout::SPACE[1]);
            widgets::text(ui, m::WELL_LIMITS, theme::chrome(size::LABEL), t.ink_muted);
        });
    });
    let rect = inner.response.rect;
    let color = if targeted { t.brass } else { t.rule_strong };
    dashed_round_rect(ui, rect, theme::RADIUS_WELL, Stroke::new(1.0, color));

    // The library slot: the line, its buttons and the line under it. Not
    // drawn for the README pictures, where it would name the capture's own
    // scratch library (display: none, as the capture's steps.js did).
    if app.picture_mode {
        return;
    }
    // The library line.
    if let Some(library) = app.convert.library.clone() {
        ui.add_space(layout::SPACE[6]);
        ui.horizontal_wrapped(|ui| {
            widgets::text(
                ui,
                &m::library_line(&library),
                theme::chrome(size::LABEL),
                t.ink_muted,
            );
            for action in m::library_actions(&library) {
                let words = if action == LibraryAction::Change {
                    "Change…"
                } else {
                    "Reset"
                };
                let enabled = !app.convert.library_busy;
                let b = widgets::button(ui, &t, Look::Quiet, words, enabled);
                app.stop(b.id, enabled);
                if b.clicked() {
                    library_run(app, action);
                }
            }
        });
    }
    if !app.convert.secondary.is_empty() {
        let line = app.convert.secondary.clone();
        widgets::text(ui, &line, theme::chrome(size::LABEL), t.ink_muted);
    }
}

/// The well's mark: a page with its corner turned and an arrow down.
fn paint_well_mark(ui: &Ui, rect: Rect, color: Color32) {
    let s = rect.width() / 26.0;
    let p = |x: f32, y: f32| rect.min + Vec2::new(x * s, y * s);
    let stroke = Stroke::new(1.3, color);
    let painter = ui.painter();
    painter.add(Shape::line(
        vec![
            p(6.0, 3.5),
            p(15.0, 3.5),
            p(20.0, 8.5),
            p(20.0, 22.5),
            p(6.0, 22.5),
            p(6.0, 3.5),
        ],
        stroke,
    ));
    painter.add(Shape::line(
        vec![p(15.0, 3.5), p(15.0, 8.5), p(20.0, 8.5)],
        stroke,
    ));
    painter.line_segment([p(13.0, 11.5), p(13.0, 18.5)], stroke);
    painter.add(Shape::line(
        vec![p(10.0, 15.5), p(13.0, 18.5), p(16.0, 15.5)],
        stroke,
    ));
}

/// A dashed border with rounded corners (the well's 1.5px dashed line).
fn dashed_round_rect(ui: &Ui, rect: Rect, radius: f32, stroke: Stroke) {
    let r = radius;
    let mut points = Vec::new();
    let corner = |c: egui::Pos2, start: f32, points: &mut Vec<egui::Pos2>| {
        for i in 0..=6 {
            let a = start + i as f32 / 6.0 * std::f32::consts::FRAC_PI_2;
            points.push(c + Vec2::new(a.cos(), a.sin()) * r);
        }
    };
    corner(
        rect.right_top() + Vec2::new(-r, r),
        -std::f32::consts::FRAC_PI_2,
        &mut points,
    );
    corner(rect.right_bottom() + Vec2::new(-r, -r), 0.0, &mut points);
    corner(
        rect.left_bottom() + Vec2::new(r, -r),
        std::f32::consts::FRAC_PI_2,
        &mut points,
    );
    corner(
        rect.left_top() + Vec2::new(r, r),
        std::f32::consts::PI,
        &mut points,
    );
    points.push(points[0]);
    ui.painter()
        .extend(Shape::dashed_line(&points, stroke, 6.0, 4.0));
}

fn draw_working(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    ui.add_space(layout::SPACE[8]);
    widgets::title(ui, &t, "Converting");
    ui.add_space(layout::SPACE[2]);
    widgets::text(
        ui,
        &format!(
            "The pages of {} reflow themselves, one scene at a time.",
            m::file_name(&app.convert.path)
        ),
        theme::chrome(size::UI),
        t.ink_soft,
    );
    ui.add_space(layout::SPACE[6]);
    ui.vertical_centered(|ui| {
        let fill = app.ease(
            Id::new(FILL),
            f32::from(app.convert.progress.percent) / 100.0,
            crate::model::motion::PROGRESS_FILL,
        );
        widgets::track(ui, &t, fill);
        ui.add_space(layout::SPACE[2]);
        let words = app
            .convert
            .waiting
            .clone()
            .unwrap_or_else(|| app.convert.progress.label.clone());
        widgets::live(
            ui,
            Id::new("read-out"),
            &words,
            theme::chrome(size::LABEL),
            t.ink_muted,
        );
    });
}

fn draw_done(app: &mut App, ui: &mut Ui, block: layout::Span) {
    let geometry = layout::result(layout::Span {
        left: 0.0,
        width: block.width,
    });
    let t = app.tokens();
    ui.add_space(layout::SPACE[8]);
    let top = ui.cursor().min;
    let page_rect = |height: f32| {
        Rect::from_min_size(
            top + Vec2::new(geometry.page.left, 0.0),
            Vec2::new(geometry.page.width, height),
        )
    };
    let page_height = geometry.page.width * 4.0 / 3.0;
    let mut side_top = top + Vec2::new(geometry.side.left, 0.0);
    let page_top_y = (!geometry.stacked).then_some(top.y);
    // The side: what is ready, the counts, the panel, the quiet links.
    let side = ui.scope_builder(
        egui::UiBuilder::new().max_rect(Rect::from_min_size(
            side_top,
            Vec2::new(geometry.side.width, f32::INFINITY),
        )),
        |ui| {
            ui.spacing_mut().item_spacing.y = layout::SPACE[2];
            let script = app.script.clone();
            let title = script.as_ref().map(|s| s.title.clone()).unwrap_or_default();
            widgets::heading(
                ui,
                &m::ready_line(&title),
                theme::chrome_strong(size::BODY),
                t.ink,
            );
            let counts = script
                .as_ref()
                .map(|s| m::counts_line(s.pages, s.scenes))
                .unwrap_or_default();
            if !counts.is_empty() {
                widgets::text(ui, &counts, theme::chrome(size::UI), t.ink_muted);
            }
            for w in script
                .as_ref()
                .map(|s| s.warnings.clone())
                .unwrap_or_default()
            {
                widgets::caption(ui, &t, &format!("({w})"), false);
            }
            ui.add_space(layout::SPACE[4] - layout::SPACE[2]);
            widgets::panel_frame(&t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                super::panel::draw(app, ui);
            });
            ui.add_space(layout::SPACE[3]);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = layout::SPACE[1];
                for (words, go) in [("Open in Read", 1), ("Settings", 2), ("Convert another", 3)] {
                    let b = widgets::button(ui, &t, Look::Quiet, words, true);
                    app.stop(b.id, true);
                    if b.clicked() {
                        match go {
                            1 => app.go_to(Surface::Read),
                            2 => app.go_to(Surface::Tune),
                            _ => reset(app),
                        }
                    }
                }
            });
        },
    );
    side_top.y = side.response.rect.bottom() + layout::SPACE[7];
    let page_y = page_top_y.unwrap_or(side_top.y);
    let rect = page_rect(page_height).translate(Vec2::new(0.0, page_y - top.y));
    first_page(app, ui, rect);
    let bottom = side.response.rect.bottom().max(rect.bottom());
    ui.allocate_rect(
        Rect::from_min_max(top, egui::pos2(top.x + block.width, bottom)),
        Sense::hover(),
    );
}

fn draw_failed(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let Some(refusal) = app.convert.refusal.clone() else {
        return;
    };
    ui.add_space(layout::SPACE[8]);
    widgets::heading(
        ui,
        refusal.heading,
        theme::chrome_strong(size::BODY),
        t.alarm,
    );
    ui.add_space(layout::SPACE[2]);
    ui.scope(|ui| {
        ui.set_max_width(66.0 * 8.0);
        widgets::text(ui, &refusal.message, theme::chrome(size::UI), t.ink);
    });
    ui.add_space(layout::SPACE[6]);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = layout::SPACE[2];
        if refusal.can_force {
            let b = widgets::button(ui, &t, Look::Brass, "Convert anyway", true);
            app.stop(b.id, true);
            if b.clicked() {
                let path = app.convert.failed_path.clone();
                convert_path(app, &path, true);
            }
        }
        let back = widgets::button(ui, &t, Look::Outline, "Back to one", true);
        app.stop(back.id, true);
        if back.clicked() {
            draw_well(app);
            app.restore_focus = true;
        }
        let bug = widgets::button(ui, &t, Look::Quiet, "Report a bug", true);
        app.stop(bug.id, true);
        if bug.clicked() {
            report(app, &refusal);
        }
    });
    if let Some(kept) = m::still_open_note(app.script.as_ref()) {
        ui.add_space(layout::SPACE[5]);
        widgets::text(ui, &kept, theme::chrome(size::LABEL), t.ink_muted);
    }
    ui.add_space(layout::SPACE[6]);
    ui.horizontal(|ui| {
        widgets::text(ui, "Error code", theme::chrome(size::LABEL), t.ink_muted);
        ui.add_space(layout::SPACE[1]);
        egui::Frame::NONE
            .fill(t.bar)
            .corner_radius(theme::RADIUS_MARK)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                widgets::text(ui, &refusal.code, theme::chrome(size::LABEL), t.ink_soft);
            });
    });
}

/// The foot of Save a file: Show in Finder, the book's path, and the
/// engine's refusal when the reveal failed (for this book only).
pub fn draw_reveal_foot(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let Some(book) = app.script.as_ref().and_then(|s| s.epub_path.clone()) else {
        return;
    };
    ui.add_space(layout::SPACE[4]);
    let b = widgets::button(
        ui,
        &t,
        Look::Quiet,
        m::reveal_label(app.host.platform),
        true,
    );
    app.stop(b.id, true);
    if b.clicked() {
        reveal(app, book.clone());
    }
    widgets::text(ui, &book, theme::chrome(size::NOTE), t.ink_muted);
    if let Some((for_book, note)) = &app.convert.reveal_note
        && *for_book == book
        && !note.is_empty()
    {
        let note = note.clone();
        widgets::caption(ui, &t, &note, false);
    }
}

/// The page show hears that the book exists once the result's first page
/// is laid out (page-show.js took bookAt when the result's frame had
/// loaded): its typed paragraphs are paired with the book's headings and
/// paragraphs as that page lays them out, and the pictures of the blocks
/// they will become are asked for ahead of the wave. A book with no pages
/// is heard at once.
pub fn adopt_when_laid(app: &mut App, ctx: &egui::Context) {
    if app.convert.view != View::Done || app.show.as_ref().is_none_or(|s| s.book_arrived()) {
        return;
    }
    let pages = app
        .script
        .as_ref()
        .is_some_and(|s| !s.preview_html.trim().is_empty());
    let words = if pages {
        match app.views.peek_fresh(Place::FirstPage) {
            Some(view) => Some(view.words()),
            None => return,
        }
    } else {
        None
    };
    let now = app.now();
    let Some(show) = app.show.as_mut() else {
        return;
    };
    show.adopt(words.as_deref(), now);
    let wanted: Vec<usize> = show
        .sources()
        .map(|s| s.iter().flatten().copied().collect())
        .unwrap_or_default();
    if let Some(view) = app.views.peek_fresh_mut(Place::FirstPage) {
        view.want_blocks(ctx, wanted);
    }
}

/// The result's first page: the book's own, opened on its first scene a
/// line and a half down; painted only (hidden from the screen reader, like
/// the frame it replaces). While the page writes itself, the typed page is
/// drawn over it.
fn first_page(app: &mut App, ui: &mut Ui, rect: Rect) {
    let (t, dark) = (app.tokens(), app.dark);
    sheet(ui, &t, rect);
    let html = app
        .script
        .as_ref()
        .map(|s| s.preview_html.clone())
        .unwrap_or_default();
    let covering = app.show.as_ref().is_some_and(|s| s.covers_page());
    let fade = app.show.as_ref().map(|s| s.page_opacity()).unwrap_or(0.0);
    if html.trim().is_empty() {
        if let Some(show) = app.show.as_ref().filter(|_| covering) {
            super::show::paint_typed(ui, &t, rect, show, None, fade);
        }
        return;
    }
    let view = app
        .views
        .get(Place::FirstPage, &html, rect.width(), &t, dark);
    let scene_top = view
        .laid()
        .and_then(|l| l.scenes.iter().find(|(_, heading)| !heading.is_empty()))
        .map(|(m, _)| m.top)
        .unwrap_or(0.0);
    let top = (scene_top - 1.5 * BOOK_EM).max(0.0);
    if !covering || fade < 1.0 {
        view.paint_slice(ui, "first-page", rect.shrink(1.0), top);
    }
    if covering && let Some(show) = app.show.as_ref() {
        super::show::paint_typed(ui, &t, rect, show, Some(view), fade);
    }
}
