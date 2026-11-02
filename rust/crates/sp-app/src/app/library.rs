//! The Library tab (docs/superpowers/specs/2026-11-02-library-view-design.md):
//! every script in the library folder, and Open, which reopens one without
//! converting its PDF again. Every rule and word is in src/model/library.rs;
//! the engine lists (`library`) and remakes the book (`convert-to` from the
//! library `.fountain`, as the sample reopens). This draws the answers.

use egui::{Id, Ui};

use super::panel::way_row;
use super::widgets::{self, Look};
use super::{App, Purpose as AppPurpose};
use crate::engine::{Reply, Request};
use crate::model::layout;
use crate::model::library::{
    self as m, Listing, Row, detail, empty_line, gone_note, listing_failure, listing_from,
    open_label, row_title,
};
use crate::theme::{self, size};

#[derive(Default)]
pub struct State {
    pub listing: Option<Listing>,
    pub looking: bool,
    pub fault: Option<String>,
    /// Each ask's number: only the latest one's answer is drawn.
    asks: u64,
}

#[derive(Clone, Debug)]
pub enum Purpose {
    List { ask: u64 },
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

pub fn answered(app: &mut App, purpose: Purpose, reply: Reply) {
    let answer = App::answer_or(&reply);
    match purpose {
        Purpose::List { ask } => {
            if ask != app.library.asks {
                return;
            }
            let s = &mut app.library;
            s.looking = false;
            match listing_from(&answer) {
                Some(listing) => {
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
    // One conversion at a time, as the Convert page has it.
    let live = !app.convert.converting;
    let mut open = None;
    way_row(
        ui,
        |ui| {
            widgets::text(ui, row_title(row), theme::chrome_strong(size::UI), t.ink);
            widgets::text(ui, &detail(row), theme::chrome(size::LABEL), t.ink_muted);
            if let Some(note) = gone_note(row) {
                widgets::caption(ui, &t, note, false);
            }
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
            super::convert::reopen(app, row.clone());
        }
    }
}
