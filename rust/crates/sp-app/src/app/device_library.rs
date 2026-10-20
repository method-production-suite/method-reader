//! What is on the reader (docs/device-library.md): the send panel's
//! "Books on your Kindle", the list it opens, and taking one book off with
//! a question first. Every rule and word is in src/model/device_library.rs;
//! the engine lists and removes (`device-books`, `device-remove`), and
//! decides what may go. This draws the answers.

use egui::{Id, Ui};

use super::panel::{keyed, way_row};
use super::widgets::{self, Look};
use super::{App, Purpose as AppPurpose};
use crate::engine::{Reply, Request};
use crate::model::device_library::{
    self as m, Book, Shelf, detail, empty, heading, looking, open_label, question, question_note,
    readers, remove_label, removed_line, removing, shelf_failure, shelf_from, still_there,
};
use crate::model::layout;
use crate::model::send_routes::Device;
use crate::theme::{self, size};

#[derive(Default)]
pub struct State {
    /// The reader whose list is open.
    pub open: Option<Device>,
    pub shelf: Option<Shelf>,
    pub looking: bool,
    pub fault: Option<String>,
    asks: u64,
    /// The book whose question is showing, by path.
    pub confirm: Option<String>,
    /// The book going now, by path.
    pub removing: Option<String>,
    /// The last thing a removal said, and whether it was bad news.
    pub said: Option<(String, bool)>,
    /// The question has just opened: the keyboard goes to its Cancel.
    focus_cancel: bool,
}

#[derive(Clone, Debug)]
pub enum Purpose {
    Books { ask: u64 },
    Remove { book: Book, device: Device },
}

fn p(purpose: Purpose) -> AppPurpose {
    AppPurpose::Shelf(purpose)
}

/// The readers whose books can be listed right now.
pub fn offered(app: &App) -> Vec<Device> {
    readers(app.devices.as_deref(), app.panel.drawn.as_ref())
}

fn same(a: &Device, b: &Device) -> bool {
    a.volume == b.volume
}

/// The button: opens this reader's list, or closes it.
fn toggle(app: &mut App, device: Device) {
    let s = &mut app.shelf;
    if s.open.as_ref().is_some_and(|o| same(o, &device)) {
        *s = State {
            asks: s.asks,
            ..State::default()
        };
        return;
    }
    s.open = Some(device);
    s.shelf = None;
    s.fault = None;
    s.confirm = None;
    s.said = None;
    look(app);
}

/// Ask the engine for the open reader's books.
fn look(app: &mut App) {
    let Some(volume) = app.shelf.open.as_ref().and_then(|d| d.volume.clone()) else {
        return;
    };
    app.shelf.asks += 1;
    app.shelf.looking = true;
    let ask = app.shelf.asks;
    app.call(Request::DeviceBooks { volume }, p(Purpose::Books { ask }));
}

fn remove(app: &mut App, book: Book) {
    let Some(device) = app.shelf.open.clone() else {
        return;
    };
    let Some(volume) = device.volume.clone() else {
        return;
    };
    app.shelf.confirm = None;
    app.shelf.removing = Some(book.path.clone());
    app.shelf.said = Some((removing(&book), false));
    app.call(
        Request::DeviceRemove {
            volume,
            file: book.path.clone(),
        },
        p(Purpose::Remove { book, device }),
    );
}

pub fn answered(app: &mut App, purpose: Purpose, reply: Reply) {
    let answer = App::answer_or(&reply);
    match purpose {
        Purpose::Books { ask } => {
            if ask != app.shelf.asks || app.shelf.open.is_none() {
                return;
            }
            app.shelf.looking = false;
            match shelf_from(&answer) {
                Some(shelf) => {
                    // A question about a book that has gone goes with it.
                    if let Some(c) = &app.shelf.confirm
                        && !shelf.books.iter().any(|b| &b.path == c)
                    {
                        app.shelf.confirm = None;
                    }
                    app.shelf.shelf = Some(shelf);
                    app.shelf.fault = None;
                }
                None => {
                    app.shelf.shelf = None;
                    app.shelf.fault = Some(shelf_failure(&answer));
                }
            }
        }
        Purpose::Remove { book, device } => {
            app.shelf.removing = None;
            let (line, bad) = removed_line(&answer, &book, &device);
            app.shelf.said = Some((line, bad));
            app.restore_focus = true;
            if app.shelf.open.as_ref().is_some_and(|o| same(o, &device)) {
                look(app);
            }
        }
    }
}

/// The devices poll heard a new list: a reader that has gone takes its
/// list with it.
pub fn devices_changed(app: &mut App, list: &[Device]) {
    if let Some(open) = &app.shelf.open
        && !still_there(open, list)
    {
        app.shelf = State {
            asks: app.shelf.asks,
            ..State::default()
        };
    }
}

/// A send finished: the open list looks again, so the new copy shows.
pub fn sent(app: &mut App) {
    if app.shelf.open.is_some() && app.shelf.removing.is_none() {
        look(app);
    }
}

/// Whether nothing is going off a reader right now.
pub fn idle(app: &App) -> bool {
    app.shelf.removing.is_none()
}

// ---------------------------------------------------------------- drawing

/// The buttons, one per reader, inside the caller's row.
pub fn entry(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    for device in offered(app) {
        let key = format!("shelf-{}", device.volume.as_deref().unwrap_or(""));
        let open = app.shelf.open.as_ref().is_some_and(|o| same(o, &device));
        let b = keyed(
            app,
            ui,
            Look::Quiet,
            &open_label(&device),
            true,
            &key,
            false,
        );
        ui.ctx()
            .accesskit_node_builder(b.id, |n| n.set_expanded(open));
        widgets::beta_badge(ui, &t);
        if b.clicked() {
            toggle(app, device);
        }
    }
}

/// The open list, under the caller's row.
pub fn draw(app: &mut App, ui: &mut Ui) {
    let Some(device) = app.shelf.open.clone() else {
        return;
    };
    let t = app.tokens();
    ui.add_space(layout::SPACE[3]);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().min.y,
        egui::Stroke::new(1.0, t.rule),
    );
    ui.add_space(layout::SPACE[4]);
    widgets::heading(ui, &heading(&device), theme::chrome_strong(size::UI), t.ink);
    if let Some(fault) = app.shelf.fault.clone() {
        widgets::text(ui, &fault, theme::chrome(size::UI), t.ink);
    } else if let Some(shelf) = app.shelf.shelf.clone() {
        if shelf.books.is_empty() {
            widgets::text(ui, &empty(&device), theme::chrome(size::UI), t.ink_muted);
        }
        let (ours, others) = shelf.groups();
        for (words, books) in [(m::YOUR_SCRIPTS, ours), (m::OTHER_BOOKS, others)] {
            if books.is_empty() {
                continue;
            }
            ui.add_space(layout::SPACE[3]);
            widgets::heading(ui, words, theme::chrome_strong(size::LABEL), t.ink_muted);
            for book in books {
                book_row(app, ui, &device, book);
            }
        }
    } else if app.shelf.looking {
        widgets::text(ui, &looking(&device), theme::chrome(size::UI), t.ink_muted);
    }
    let (said, bad) = app.shelf.said.clone().unwrap_or_default();
    widgets::live(
        ui,
        Id::new("shelf-status"),
        &said,
        theme::chrome(size::LABEL),
        if bad { t.alarm } else { t.ink_muted },
    );
}

fn book_row(app: &mut App, ui: &mut Ui, device: &Device, book: &Book) {
    let t = app.tokens();
    let asking = app.shelf.confirm.as_deref() == Some(book.path.as_str());
    let live = idle(app) && !app.panel.sending;
    let mut ask = false;
    let app_cell = std::cell::RefCell::new(&mut *app);
    way_row(
        ui,
        |ui| {
            widgets::text(ui, &book.name, theme::chrome_strong(size::UI), t.ink);
            widgets::text(ui, &detail(book), theme::chrome(size::LABEL), t.ink_muted);
        },
        |ui| {
            if !asking {
                let mut app = app_cell.borrow_mut();
                let key = format!("shelf-remove-{}", book.path);
                let b = keyed(&mut app, ui, Look::Outline, "Remove…", live, &key, false);
                let label = remove_label(book);
                ui.ctx()
                    .accesskit_node_builder(b.id, |n| n.set_label(label));
                ask = b.clicked();
            }
        },
    );
    if ask {
        app.shelf.confirm = Some(book.path.clone());
        app.shelf.said = None;
        app.shelf.focus_cancel = true;
    }
    if asking {
        confirm(app, ui, device, book);
    }
}

/// The question, under its book: what goes, and the two answers.
fn confirm(app: &mut App, ui: &mut Ui, device: &Device, book: &Book) {
    let t = app.tokens();
    widgets::prose(ui, &t, &question(book, device));
    if let Some(note) = question_note(device) {
        widgets::caption(ui, &t, note, false);
    }
    let live = idle(app) && !app.panel.sending;
    let mut yes = false;
    let mut no = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = layout::SPACE[3];
        let r = keyed(app, ui, Look::Outline, m::REMOVE, live, "shelf-yes", false);
        let label = format!("{} {}", m::REMOVE, book.name);
        ui.ctx()
            .accesskit_node_builder(r.id, |n| n.set_label(label));
        let c = keyed(app, ui, Look::Quiet, m::CANCEL, true, "shelf-no", false);
        if app.shelf.focus_cancel {
            app.shelf.focus_cancel = false;
            c.request_focus();
        }
        yes = r.clicked();
        no = c.clicked();
    });
    ui.add_space(layout::SPACE[2]);
    if yes {
        remove(app, book.clone());
    } else if no {
        app.shelf.confirm = None;
        app.restore_focus = true;
    }
}
