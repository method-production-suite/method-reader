//! The Library tab (docs/superpowers/specs/2026-11-02-library-view-design.md):
//! every script in the library folder, Open, which reopens one without
//! converting its PDF again, and Remove from library, which asks first and
//! then moves the script's folder to the Trash
//! (docs/superpowers/specs/2026-11-03-library-remove-design.md), and
//! Settings, which reopens it as Open does and lands on the Settings tab
//! (docs/superpowers/specs/2026-11-03-library-row-settings-design.md), and
//! Rename, a one-line field for the name the script goes by
//! (docs/superpowers/specs/2026-11-03-library-rename-design.md), and
//! Report, the script's length and who speaks how much, under its row
//! (docs/superpowers/specs/2026-11-04-conversion-report-design.md). Every rule
//! and word is in src/model/library.rs; the engine lists (`library`),
//! remakes the book (`convert-to` from the library `.fountain`, as the
//! sample reopens), removes (`library-remove`), renames
//! (`library-rename`) and reports (`library-report`). This draws the
//! answers.

use egui::{Id, Key, Ui};

use super::panel::way_row;
use super::widgets::{self, Look};
use super::{App, Purpose as AppPurpose};
use crate::engine::{Reply, Request};
use crate::model::convert::{Platform, reveal_failure_message};
use crate::model::frame::Surface;
use crate::model::layout;
use crate::model::library::{
    self as m, Listing, MadeBook, Report, Row, detail, empty_line, gone_note, hide_report_label,
    lines_from, listing_failure, listing_from, made_book, make_lines_label, make_sides_label,
    making_lines, making_sides, move_button, move_label, moving, name_field_label, open_label,
    question, question_note, remove_label, removed_line, rename_label, renamed_line, renaming,
    report_from, report_label, report_table_label, reporting, row_title, save_label,
    settings_label, sides_from,
};
use crate::model::send_routes::Status;
use crate::theme::{self, size};

#[derive(Default)]
pub struct State {
    pub listing: Option<Listing>,
    pub looking: bool,
    pub fault: Option<String>,
    /// Each ask's number: only the latest one's answer is drawn.
    asks: u64,
    /// The script whose question is showing, by folder.
    pub confirm: Option<String>,
    /// The script going now, by folder.
    pub removing: Option<String>,
    /// The last thing a removal said, and whether it was bad news.
    pub said: Option<(String, bool)>,
    /// The question has just opened: the keyboard goes to its Cancel.
    focus_cancel: bool,
    /// The script whose rename field is showing, by folder.
    pub editing: Option<String>,
    /// What the rename field holds.
    pub draft: String,
    /// The script being renamed now, by folder.
    pub renaming: Option<String>,
    /// The rename field has just opened: the keyboard goes to it.
    focus_field: bool,
    /// The script whose report is showing (or being read), by folder: one
    /// at a time.
    pub report_for: Option<String>,
    /// Its report, or why there is none; `None` while it is being read.
    pub report: Option<Result<Report, String>>,
    /// Each report ask's number: only the latest one's answer is drawn.
    report_asks: u64,
    /// The character whose sides are being made now.
    pub making_sides: Option<String>,
    /// The book last made for the report showing.
    pub sides_file: Option<MadeBook>,
}

#[derive(Clone, Debug)]
pub enum Purpose {
    List {
        ask: u64,
    },
    /// Boxed: a row is many times a list's ask.
    Remove(Box<Row>),
    /// The row, and the name it was sent (empty: cleared).
    Rename(Box<Row>, String),
    /// The row whose report was asked for, and which ask it was.
    Report(Box<Row>, u64),
    /// The row, and the character whose sides were asked for.
    Sides(Box<Row>, String),
    /// The row, and the character whose lines in bold were asked for.
    Lines(Box<Row>, String),
    /// The sides just made, shown in the file manager.
    RevealSides,
}

/// The tab is showing: the folder may have changed since, so ask again.
pub fn shown(app: &mut App) {
    look(app);
}

/// Ask the engine for the library's scripts.
pub fn look(app: &mut App) {
    app.library.asks += 1;
    app.library.looking = true;
    let ask = app.library.asks;
    app.call(Request::Library, AppPurpose::Library(Purpose::List { ask }));
}

/// Whether the Trash words are Windows' (the Recycle Bin).
fn windows(app: &App) -> bool {
    app.host.platform == Platform::Windows
}

/// Move to Trash: one `library-remove` for the row's folder.
fn remove(app: &mut App, row: Row) {
    let said = moving(&row, windows(app));
    let s = &mut app.library;
    s.confirm = None;
    s.removing = Some(row.folder.clone());
    s.said = Some((said, false));
    app.call(
        Request::LibraryRemove {
            folder: row.folder.clone(),
        },
        AppPurpose::Library(Purpose::Remove(Box::new(row))),
    );
}

/// Save: one `library-rename` for the row's folder and the field's name.
fn rename(app: &mut App, row: Row) {
    let name = app.library.draft.trim().to_string();
    let s = &mut app.library;
    s.renaming = Some(row.folder.clone());
    s.said = Some((renaming(&row), false));
    app.call(
        Request::LibraryRename {
            folder: row.folder.clone(),
            name: name.clone(),
        },
        AppPurpose::Library(Purpose::Rename(Box::new(row), name)),
    );
}

/// Report: one `library-report` for the row's folder, its answer shown
/// under the row.
fn report(app: &mut App, row: Row) {
    let s = &mut app.library;
    s.report_asks += 1;
    s.report_for = Some(row.folder.clone());
    s.report = None;
    s.sides_file = None;
    let ask = s.report_asks;
    app.call(
        Request::LibraryReport {
            folder: row.folder.clone(),
        },
        AppPurpose::Library(Purpose::Report(Box::new(row), ask)),
    );
}

/// Close the report, if it is `folder`'s (any, for `None`).
fn close_report(app: &mut App, folder: Option<&str>) {
    let s = &mut app.library;
    if folder.is_none() || s.report_for.as_deref() == folder {
        s.report_for = None;
        s.report = None;
        s.sides_file = None;
    }
}

/// Make `name`'s sides, as the report on `row` lists them.
fn make_sides(app: &mut App, row: Row, name: String) {
    let s = &mut app.library;
    s.making_sides = Some(name.clone());
    s.sides_file = None;
    s.said = Some((making_sides(&name), false));
    app.call(
        Request::LibrarySides {
            folder: row.folder.clone(),
            name: name.clone(),
        },
        AppPurpose::Library(Purpose::Sides(Box::new(row), name)),
    );
}

fn make_lines(app: &mut App, row: Row, name: String) {
    let s = &mut app.library;
    s.making_sides = Some(name.clone());
    s.sides_file = None;
    s.said = Some((making_lines(&name), false));
    app.call(
        Request::LibraryLines {
            folder: row.folder.clone(),
            name: name.clone(),
        },
        AppPurpose::Library(Purpose::Lines(Box::new(row), name)),
    );
}

/// Whether nothing is going to the Trash, being renamed or having its
/// sides made, right now.
pub fn idle(app: &App) -> bool {
    app.library.removing.is_none()
        && app.library.renaming.is_none()
        && app.library.making_sides.is_none()
}

pub fn answered(app: &mut App, purpose: Purpose, reply: Reply) {
    let answer = App::answer_or(&reply);
    match purpose {
        Purpose::Report(row, ask) => {
            if ask != app.library.report_asks {
                return;
            }
            app.library.report = Some(report_from(&answer, &row));
        }
        Purpose::Sides(row, name) => {
            app.library.making_sides = None;
            app.library.said = Some(match sides_from(&answer, &row, &name) {
                Ok((line, epub)) => {
                    if app.library.report_for.as_deref() == Some(row.folder.as_str()) {
                        app.library.sides_file = Some(made_book(&answer, epub));
                    }
                    (line, false)
                }
                Err(why) => (why, true),
            });
        }
        Purpose::Lines(row, name) => {
            app.library.making_sides = None;
            app.library.said = Some(match lines_from(&answer, &row, &name) {
                Ok((line, epub)) => {
                    if app.library.report_for.as_deref() == Some(row.folder.as_str()) {
                        app.library.sides_file = Some(made_book(&answer, epub));
                    }
                    (line, false)
                }
                Err(why) => (why, true),
            });
        }
        Purpose::RevealSides => {
            if let Some(why) = reveal_failure_message(&answer) {
                app.library.said = Some((why, true));
            }
        }
        Purpose::Remove(row) => {
            close_report(app, Some(&row.folder));
            app.library.removing = None;
            app.library.said = Some(removed_line(&answer, &row, windows(app)));
            app.restore_focus = true;
            look(app);
        }
        Purpose::Rename(row, name) => {
            app.library.renaming = None;
            let (line, bad) = renamed_line(&answer, &row, &name);
            if !bad {
                // Its report carried the old name.
                close_report(app, Some(&row.folder));
                app.library.editing = None;
                app.restore_focus = true;
                // The script open is this one: it keeps its place, called
                // by its new name; the engine titles its next book so.
                if let Some(title) = crate::model::json::text_at(&answer, "title")
                    && let Some(script) = app.script.as_mut()
                    && script.fountain_path.as_deref() == Some(row.fountain.as_str())
                {
                    script.title = title;
                }
            }
            app.library.said = Some((line, bad));
            look(app);
        }
        Purpose::List { ask } => {
            if ask != app.library.asks {
                return;
            }
            let s = &mut app.library;
            s.looking = false;
            match listing_from(&answer) {
                Some(listing) => {
                    // A question or a field about a script that has gone
                    // goes with it.
                    let gone = |f: &Option<String>| {
                        f.as_ref()
                            .is_some_and(|f| !listing.rows.iter().any(|r| &r.folder == f))
                    };
                    if gone(&s.confirm) {
                        s.confirm = None;
                    }
                    if gone(&s.editing) {
                        s.editing = None;
                    }
                    if gone(&s.report_for) {
                        s.report_for = None;
                        s.report = None;
                        s.sides_file = None;
                    }
                    s.listing = Some(listing);
                    s.fault = None;
                }
                None => {
                    s.listing = None;
                    s.fault = Some(listing_failure(&answer));
                }
            }
        }
    }
}

// ---------------------------------------------------------------- drawing

pub fn draw(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    widgets::title(ui, &t, m::TITLE);
    ui.add_space(layout::SPACE[3]);
    let (said, bad) = app.library.said.clone().unwrap_or_default();
    let status = Status {
        line: said.clone(),
        bad,
        working: !idle(app),
    };
    let (now, reduced) = (app.now(), app.reduced_motion());
    if widgets::status_line(ui, &t, Id::unique("library-status"), &status, now, reduced) {
        app.moving();
    }
    if !said.is_empty() {
        ui.add_space(layout::SPACE[3]);
    }
    if let Some(fault) = app.library.fault.clone() {
        widgets::prose(ui, &t, &fault);
        return;
    }
    match app.library.listing.clone() {
        Some(listing) if listing.rows.is_empty() => {
            widgets::prose(ui, &t, m::EMPTY);
            let home = app.convert.known_home.clone();
            widgets::prose(ui, &t, &empty_line(&listing.root, home.as_deref()));
        }
        Some(listing) => {
            for row in &listing.rows {
                script_row(app, ui, row);
                ui.add_space(layout::SPACE[3]);
            }
        }
        None if app.library.looking => {
            widgets::caption(ui, &t, m::LOOKING, false);
        }
        None => {}
    }
}

fn script_row(app: &mut App, ui: &mut Ui, row: &Row) {
    let t = app.tokens();
    // One conversion at a time, as the Convert page has it, and one
    // removal or rename at a time; the script going cannot be opened on
    // its way, nor the one whose book is being made again.
    let this = Some(row.folder.as_str());
    let going = app.library.removing.as_deref() == this || app.library.renaming.as_deref() == this;
    let live = !app.convert.converting && !going;
    let can_remove = !app.convert.converting && idle(app);
    let asking = app.library.confirm.as_deref() == Some(row.folder.as_str());
    let editing = app.library.editing.as_deref() == Some(row.folder.as_str());
    let reporting_this = app.library.report_for.as_deref() == this;
    let mut open = None;
    let mut facts = None;
    let mut tune = None;
    let mut ask = None;
    let mut name = None;
    way_row(
        ui,
        |ui| {
            widgets::text(ui, row_title(row), theme::chrome_strong(size::UI), t.ink);
            widgets::text(ui, &detail(row), theme::chrome(size::LABEL), t.ink_muted);
            if let Some(note) = gone_note(row) {
                widgets::caption(ui, &t, note, false);
            }
            // Settings, Rename and Remove on one line when the column has
            // room for them, wrapped onto the next when it has not.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = layout::SPACE[3];
                let id = Id::unique(("library-settings", row.folder.as_str()));
                let b = widgets::button_id(ui, &t, Look::Quiet, m::SETTINGS, live, id);
                let label = settings_label(row);
                ui.ctx()
                    .accesskit_node_builder(b.id, |n| n.set_label(label));
                tune = Some((b.id, b.clicked()));
                let (word, label) = if reporting_this {
                    (m::HIDE_REPORT, hide_report_label(row))
                } else {
                    (m::REPORT, report_label(row))
                };
                let id = Id::unique(("library-report", row.folder.as_str()));
                let b = widgets::button_id(ui, &t, Look::Quiet, word, !going, id);
                ui.ctx()
                    .accesskit_node_builder(b.id, |n| n.set_label(label));
                facts = Some((b.id, b.clicked()));
                if !editing {
                    let id = Id::unique(("library-rename", row.folder.as_str()));
                    let b = widgets::button_id(ui, &t, Look::Quiet, m::RENAME, can_remove, id);
                    let label = rename_label(row);
                    ui.ctx()
                        .accesskit_node_builder(b.id, |n| n.set_label(label));
                    name = Some((b.id, b.clicked()));
                }
                if !asking {
                    let id = Id::unique(("library-remove", row.folder.as_str()));
                    let b = widgets::button_id(ui, &t, Look::Quiet, m::REMOVE, can_remove, id);
                    let label = remove_label(row);
                    ui.ctx()
                        .accesskit_node_builder(b.id, |n| n.set_label(label));
                    ask = Some((b.id, b.clicked()));
                }
            });
        },
        |ui| {
            let id = Id::unique(("library-open", row.folder.as_str()));
            let b = widgets::button_id(ui, &t, Look::Outline, m::OPEN, live, id);
            let label = open_label(row);
            ui.ctx()
                .accesskit_node_builder(b.id, |n| n.set_label(label));
            open = Some((b.id, b.clicked()));
        },
    );
    if let Some((id, clicked)) = open {
        app.stop(id, live);
        if clicked && live {
            super::convert::reopen(app, row.clone(), Surface::Read);
            return;
        }
    }
    if let Some((id, clicked)) = tune {
        app.stop(id, live);
        if clicked && live {
            super::convert::reopen(app, row.clone(), Surface::Tune);
            return;
        }
    }
    if let Some((id, clicked)) = facts {
        app.stop(id, !going);
        if clicked && !going {
            if reporting_this {
                close_report(app, None);
                app.restore_focus = true;
            } else {
                report(app, row.clone());
            }
        }
    }
    if let Some((id, clicked)) = name {
        app.stop(id, can_remove);
        if clicked && can_remove {
            let s = &mut app.library;
            s.editing = Some(row.folder.clone());
            s.draft = row_title(row).to_string();
            s.confirm = None;
            s.said = None;
            s.focus_field = true;
        }
    }
    if let Some((id, clicked)) = ask {
        app.stop(id, can_remove);
        if clicked && can_remove {
            let s = &mut app.library;
            s.confirm = Some(row.folder.clone());
            s.editing = None;
            s.said = None;
            s.focus_cancel = true;
        }
    }
    if asking {
        confirm(app, ui, row);
    }
    if editing {
        name_field(app, ui, row);
    }
    if app.library.report_for.as_deref() == this {
        report_view(app, ui, row);
    }
}

/// The report, under its script: a line of totals, then who speaks, most
/// first, as a table.
fn report_view(app: &mut App, ui: &mut Ui, row: &Row) {
    let t = app.tokens();
    ui.add_space(layout::SPACE[2]);
    let report = match app.library.report.clone() {
        None => {
            widgets::caption(ui, &t, &reporting(row), false);
            return;
        }
        Some(Err(why)) => {
            widgets::prose(ui, &t, &why);
            return;
        }
        Some(Ok(report)) => report,
    };
    widgets::text(ui, &report.totals, theme::chrome(size::LABEL), t.ink_muted);
    if report.characters.is_empty() {
        return;
    }
    ui.add_space(layout::SPACE[2]);
    let live = idle(app);
    let mut make = None;
    let mut make_bold = None;
    let grid = egui::Grid::new(("library-report-table", row.folder.as_str()))
        .num_columns(m::REPORT_COLUMNS.len())
        .spacing([layout::SPACE[4], layout::SPACE[1]])
        .show(ui, |ui| {
            for heading in m::REPORT_COLUMNS {
                widgets::text(ui, heading, theme::chrome_strong(size::LABEL), t.ink);
            }
            ui.end_row();
            for c in &report.characters {
                widgets::text(ui, &c.name, theme::chrome(size::LABEL), t.ink);
                for cell in [&c.speeches, &c.words, &c.scenes, &c.first] {
                    widgets::text(ui, cell, theme::chrome(size::LABEL), t.ink_muted);
                }
                let id = Id::unique(("library-sides", row.folder.as_str(), c.name.as_str()));
                let b = widgets::button_id(ui, &t, Look::Quiet, m::MAKE_SIDES, live, id);
                let label = make_sides_label(&c.name);
                ui.ctx()
                    .accesskit_node_builder(b.id, |n| n.set_label(label));
                if b.clicked() {
                    make = Some(c.name.clone());
                }
                let id = Id::unique(("library-lines", row.folder.as_str(), c.name.as_str()));
                let b = widgets::button_id(ui, &t, Look::Quiet, m::MAKE_SIDES, live, id);
                let label = make_lines_label(&c.name);
                ui.ctx()
                    .accesskit_node_builder(b.id, |n| n.set_label(label));
                if b.clicked() {
                    make_bold = Some(c.name.clone());
                }
                ui.end_row();
            }
        });
    let label = report_table_label(row);
    ui.ctx().accesskit_node_builder(grid.response.id, |n| {
        n.set_role(egui::accesskit::Role::Table);
        n.set_label(label);
    });
    ui.add_space(layout::SPACE[2]);
    if let Some(book) = app.library.sides_file.clone() {
        let mut send = false;
        let mut show = false;
        ui.horizontal_wrapped(|ui| {
            let id = Id::unique(("library-sides-send", row.folder.as_str()));
            let free = live && !app.convert.converting;
            send = widgets::button_id(ui, &t, Look::Outline, m::SEND_MADE, free, id).clicked();
            let id = Id::unique(("library-sides-show", row.folder.as_str()));
            show = widgets::button_id(ui, &t, Look::Outline, m::SHOW_SIDES, true, id).clicked();
        });
        if show {
            app.call(
                Request::Reveal {
                    path: book.epub.clone(),
                },
                AppPurpose::Library(Purpose::RevealSides),
            );
        }
        if send {
            super::convert::open_made_book(app, &book);
        }
        ui.add_space(layout::SPACE[2]);
    }
    if let Some(name) = make {
        make_sides(app, row.clone(), name);
    }
    if let Some(name) = make_bold {
        make_lines(app, row.clone(), name);
    }
}

/// The rename field, under its script: the name it goes by now, Save and
/// Cancel, the keyboard in the field. Return saves, Escape cancels.
fn name_field(app: &mut App, ui: &mut Ui, row: &Row) {
    let t = app.tokens();
    ui.add_space(layout::SPACE[2]);
    let live = !app.convert.converting && idle(app);
    let mut field = None;
    let mut save = None;
    let mut cancel = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = layout::SPACE[3];
        let f = ui.add(
            egui::TextEdit::singleline(&mut app.library.draft)
                .id_salt(("library-name", row.folder.as_str()))
                .char_limit(m::MAX_NAME)
                .font(theme::chrome(size::UI))
                .desired_width(260.0),
        );
        let label = name_field_label(row);
        ui.ctx()
            .accesskit_node_builder(f.id, |n| n.set_label(label));
        if app.library.focus_field {
            app.library.focus_field = false;
            f.request_focus();
        }
        let id = Id::unique(("library-name-save", row.folder.as_str()));
        let s = widgets::button_id(ui, &t, Look::Outline, m::SAVE, live, id);
        let label = save_label(row);
        ui.ctx()
            .accesskit_node_builder(s.id, |n| n.set_label(label));
        let id = Id::unique(("library-name-cancel", row.folder.as_str()));
        let c = widgets::button_id(ui, &t, Look::Quiet, m::CANCEL, true, id);
        let (enter, escape) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        let typing = f.has_focus() || f.lost_focus();
        field = Some((f.id, typing && enter, typing && escape));
        save = Some((s.id, s.clicked()));
        cancel = Some((c.id, c.clicked()));
    });
    ui.add_space(layout::SPACE[2]);
    let (Some((field_id, entered, escaped)), Some((save_id, saved)), Some((cancel_id, cancelled))) =
        (field, save, cancel)
    else {
        return;
    };
    app.stop(field_id, true);
    app.stop(save_id, live);
    app.stop(cancel_id, true);
    if (saved || entered) && live {
        rename(app, row.clone());
    } else if cancelled || escaped {
        app.library.editing = None;
        app.restore_focus = true;
    }
}

/// The question, under its script: what goes and what stays, and the two
/// answers, the keyboard on Cancel.
fn confirm(app: &mut App, ui: &mut Ui, row: &Row) {
    let t = app.tokens();
    ui.add_space(layout::SPACE[2]);
    let windows = windows(app);
    widgets::prose(ui, &t, &question(row, windows));
    widgets::caption(ui, &t, &question_note(windows), false);
    let live = !app.convert.converting && idle(app);
    let mut yes = None;
    let mut no = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = layout::SPACE[3];
        let id = Id::unique(("library-move", row.folder.as_str()));
        let r = widgets::button_id(ui, &t, Look::Outline, &move_button(windows), live, id);
        let label = move_label(row, windows);
        ui.ctx()
            .accesskit_node_builder(r.id, |n| n.set_label(label));
        let id = Id::unique(("library-cancel", row.folder.as_str()));
        let c = widgets::button_id(ui, &t, Look::Quiet, m::CANCEL, true, id);
        if app.library.focus_cancel {
            app.library.focus_cancel = false;
            c.request_focus();
        }
        yes = Some((r.id, r.clicked()));
        no = Some((c.id, c.clicked()));
    });
    ui.add_space(layout::SPACE[2]);
    let (Some((yes_id, yes)), Some((no_id, no))) = (yes, no) else {
        return;
    };
    app.stop(yes_id, live);
    app.stop(no_id, true);
    if yes && live {
        remove(app, row.clone());
    } else if no {
        app.library.confirm = None;
        app.restore_focus = true;
    }
}
