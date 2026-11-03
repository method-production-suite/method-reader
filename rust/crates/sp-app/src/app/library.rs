//! The Library tab (docs/superpowers/specs/2026-11-02-library-view-design.md):
//! every script in the library folder, Open, which reopens one without
//! converting its PDF again, and Remove from library, which asks first and
//! then moves the script's folder to the Trash
//! (docs/superpowers/specs/2026-11-03-library-remove-design.md), and
//! Settings, which reopens it as Open does and lands on the Settings tab
//! (docs/superpowers/specs/2026-11-03-library-row-settings-design.md). Every rule
//! and word is in src/model/library.rs; the engine lists (`library`),
//! remakes the book (`convert-to` from the library `.fountain`, as the
//! sample reopens) and removes (`library-remove`). This draws the answers.

use egui::{Id, Ui};

use super::panel::way_row;
use super::widgets::{self, Look};
use super::{App, Purpose as AppPurpose};
use crate::engine::{Reply, Request};
use crate::model::convert::Platform;
use crate::model::frame::Surface;
use crate::model::layout;
use crate::model::library::{
    self as m, Listing, Row, detail, empty_line, gone_note, listing_failure, listing_from,
    move_button, move_label, moving, open_label, question, question_note, remove_label,
    removed_line, row_title, settings_label,
};
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
}

#[derive(Clone, Debug)]
pub enum Purpose {
    List {
        ask: u64,
    },
    /// Boxed: a row is many times a list's ask.
    Remove(Box<Row>),
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

/// Whether nothing is going to the Trash right now.
pub fn idle(app: &App) -> bool {
    app.library.removing.is_none()
}

pub fn answered(app: &mut App, purpose: Purpose, reply: Reply) {
    let answer = App::answer_or(&reply);
    match purpose {
        Purpose::Remove(row) => {
            app.library.removing = None;
            app.library.said = Some(removed_line(&answer, &row, windows(app)));
            app.restore_focus = true;
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
                    // A question about a script that has gone goes with it.
                    if let Some(c) = &s.confirm
                        && !listing.rows.iter().any(|r| &r.folder == c)
                    {
                        s.confirm = None;
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
    widgets::live(
        ui,
        Id::unique("library-status"),
        &said,
        theme::chrome(size::LABEL),
        if bad { t.alarm } else { t.ink_muted },
    );
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
    // removal at a time; the script going cannot be opened on its way.
    let going = app.library.removing.as_deref() == Some(row.folder.as_str());
    let live = !app.convert.converting && !going;
    let can_remove = !app.convert.converting && idle(app);
    let asking = app.library.confirm.as_deref() == Some(row.folder.as_str());
    let mut open = None;
    let mut tune = None;
    let mut ask = None;
    way_row(
        ui,
        |ui| {
            widgets::text(ui, row_title(row), theme::chrome_strong(size::UI), t.ink);
            widgets::text(ui, &detail(row), theme::chrome(size::LABEL), t.ink_muted);
            if let Some(note) = gone_note(row) {
                widgets::caption(ui, &t, note, false);
            }
            // Settings and Remove on one line when the column has room
            // for both, one under the other when it has not.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = layout::SPACE[3];
                let id = Id::unique(("library-settings", row.folder.as_str()));
                let b = widgets::button_id(ui, &t, Look::Quiet, m::SETTINGS, live, id);
                let label = settings_label(row);
                ui.ctx()
                    .accesskit_node_builder(b.id, |n| n.set_label(label));
                tune = Some((b.id, b.clicked()));
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
    if let Some((id, clicked)) = ask {
        app.stop(id, can_remove);
        if clicked && can_remove {
            let s = &mut app.library;
            s.confirm = Some(row.folder.clone());
            s.said = None;
            s.focus_cancel = true;
        }
    }
    if asking {
        confirm(app, ui, row);
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
