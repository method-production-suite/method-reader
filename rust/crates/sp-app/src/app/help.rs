//! The Help menu's half in the window: what each item does
//! ([`choose`]), the sheets it opens (About, Keyboard Shortcuts, Licenses
//! and Notices, Check for Updates…), and on Windows and Linux the menu
//! itself, at the foot of the page beside the version stamp ([`menu`]).
//! On a Mac the menu is the menu bar's (menu.rs), which hands its choices
//! here. What the menu holds is model/help.rs.

use egui::accesskit;
use egui::{Id, Sense, Ui, Vec2, WidgetInfo, WidgetType};

use super::widgets::{self, Look};
use super::{App, frame};
use crate::model::convert::Platform;
use crate::model::feedback::{new_issue_url, os_label};
use crate::model::help::{
    ABOUT_LINE, COPYRIGHT, HelpItem, LICENCE_LINE, NOTICES, RELEASES, SUITE, USER_GUIDE, WEBSITE,
    WEBSITE_WORDS, groups, shortcuts,
};
use crate::model::layout;
use crate::model::motion;
use crate::model::names::PRODUCT;
use crate::model::notes::RELEASE;
use crate::theme::{self, Tokens, size};

/// A sheet the Help menu opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    About,
    Shortcuts,
    Licenses,
    Updates,
}

impl Sheet {
    /// The sheet's name, read out as the dialog's.
    pub fn title(self) -> String {
        match self {
            Sheet::About => HelpItem::About.label(),
            Sheet::Shortcuts => HelpItem::Shortcuts.label(),
            Sheet::Licenses => HelpItem::Licenses.label(),
            Sheet::Updates => "Updates".into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct State {
    /// The sheet open, if one is.
    pub sheet: Option<Sheet>,
    /// Which licence or notice the Licenses sheet shows.
    pub notice: usize,
}

/// Open `sheet`, closing the release notes: one sheet at a time.
pub fn open(app: &mut App, sheet: Sheet) {
    app.notes_open = false;
    app.help.sheet = Some(sheet);
}

/// What a Help item does.
pub fn choose(app: &mut App, item: HelpItem) {
    match item {
        HelpItem::About => open(app, Sheet::About),
        HelpItem::UserGuide => {
            app.host.open(USER_GUIDE);
        }
        HelpItem::Shortcuts => open(app, Sheet::Shortcuts),
        HelpItem::WhatsNew => {
            app.help.sheet = None;
            app.notes_open = true;
        }
        HelpItem::CheckForUpdates => {
            open(app, Sheet::Updates);
            if app.update.usable(&app.host) {
                app.update.drawn.drew_offer = true;
                app.update.check(&mut app.host, true);
            }
        }
        HelpItem::AutomaticUpdates => {
            if app.update.usable(&app.host) {
                let on = !app.update.automatic(&app.host);
                app.update.set_automatic(&mut app.host, on);
            }
        }
        HelpItem::ReportBug => {
            let url = new_issue_url(
                RELEASE.version,
                &os_label(app.host.platform.word()),
                None,
                None,
            );
            app.host.open(&url);
        }
        HelpItem::Licenses => open(app, Sheet::Licenses),
        HelpItem::Suite => {
            app.host.open(SUITE);
        }
    }
}

// ------------------------------------------------------------------ menu

/// The Help menu in the window, for Windows and Linux (a Mac has the menu
/// bar's): a button in the stamp's type that drops the menu down.
pub fn menu(app: &mut App, ui: &mut Ui, t: &Tokens) {
    if app.host.platform == Platform::Mac {
        return;
    }
    let words = "Help";
    let font = theme::chrome(size::NOTE);
    let galley = ui
        .painter()
        .layout_no_wrap(words.to_string(), font, t.ink_muted);
    let (rect, button) = ui.allocate_exact_size(galley.size(), Sense::click());
    button.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, words));
    ui.ctx().accesskit_node_builder(button.id, |n| {
        n.set_has_popup(accesskit::HasPopup::Menu);
    });
    let ink = if button.hovered() { t.ink } else { t.ink_muted };
    ui.painter()
        .galley_with_override_text_color(rect.min, galley, ink);
    widgets::focus_ring(ui, t, &button, 3.0);
    let usable = app.update.usable(&app.host);
    let automatic = app.update.automatic(&app.host);
    let mut chosen = None;
    egui::Popup::menu(&button)
        .align(egui::RectAlign::TOP_END)
        .show(|ui| {
            ui.ctx().accesskit_node_builder(ui.id(), |n| {
                n.set_role(accesskit::Role::Menu);
                n.set_label("Help");
            });
            for (i, group) in groups(app.host.platform).into_iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                for item in group {
                    let label = item.label();
                    let r = if item == HelpItem::AutomaticUpdates {
                        let mut on = usable && automatic;
                        let r = ui.add_enabled(usable, egui::Checkbox::new(&mut on, &label));
                        ui.ctx().accesskit_node_builder(r.id, |n| {
                            n.set_role(accesskit::Role::MenuItemCheckBox);
                        });
                        r
                    } else {
                        let r = ui.button(&label);
                        ui.ctx().accesskit_node_builder(r.id, |n| {
                            n.set_role(accesskit::Role::MenuItem);
                        });
                        r
                    };
                    if r.clicked() {
                        chosen = Some(item);
                    }
                }
            }
        });
    if let Some(item) = chosen {
        egui::Popup::close_all(ui.ctx());
        choose(app, item);
    }
}

// ---------------------------------------------------------------- sheets

/// The open Help sheet, over the page like the release notes.
pub fn draw(app: &mut App, ui: &mut Ui) {
    let motion_id = Id::new("help-sheet-rise");
    let Some(sheet) = app.help.sheet else {
        app.forget_motion(motion_id);
        return;
    };
    let t = app.tokens();
    let ctx = ui.ctx().clone();
    let k = app.ease_from(motion_id, 0.0, 1.0, motion::SHEET_RISE);
    let modal_id = Id::new(("help-sheet", sheet.title()));
    let area = egui::Modal::default_area(modal_id).fade_in(false).anchor(
        egui::Align2::CENTER_CENTER,
        Vec2::new(0.0, (1.0 - k) * motion::SHEET_RISE_PX),
    );
    let modal = egui::Modal::new(modal_id)
        .area(area)
        .frame(frame::sheet_frame(&t, k))
        .backdrop_color(frame::sheet_backdrop(&t))
        .show(&ctx, |ui| {
            ui.multiply_opacity(k);
            let width = (44.0 * 16.0f32).min(ctx.content_rect().width() * 0.88);
            ui.set_width(width);
            let title = sheet.title();
            ui.ctx().accesskit_node_builder(ui.id(), |n| {
                n.set_role(accesskit::Role::Dialog);
                n.set_label(title.as_str());
            });
            let head = match sheet {
                Sheet::About => format!("{PRODUCT} {}", RELEASE.version),
                Sheet::Shortcuts => HelpItem::Shortcuts.label(),
                Sheet::Licenses => HelpItem::Licenses.label(),
                Sheet::Updates => "Updates".to_string(),
            };
            let close = frame::sheet_head(ui, &t, &head);
            egui::ScrollArea::vertical()
                .id_salt("help-sheet-scroll")
                .max_height(ctx.content_rect().height() * 0.82 - 80.0)
                .show(ui, |ui| match sheet {
                    Sheet::About => about(app, ui, &t),
                    Sheet::Shortcuts => keys(app, ui, &t),
                    Sheet::Licenses => licences(app, ui, &t),
                    Sheet::Updates => updates(app, ui, &t),
                });
            close
        });
    if modal.inner || modal.should_close() {
        // A button inside may have opened another sheet; only this one goes.
        if app.help.sheet == Some(sheet) {
            app.help.sheet = None;
        }
        app.restore_focus = true;
    }
}

fn about(app: &mut App, ui: &mut Ui, t: &Tokens) {
    widgets::prose(ui, t, ABOUT_LINE);
    ui.add_space(layout::SPACE[3]);
    if widgets::button(ui, t, Look::Outline, WEBSITE_WORDS, true).clicked() {
        app.host.open(WEBSITE);
    }
    ui.add_space(layout::SPACE[4]);
    widgets::prose(ui, t, LICENCE_LINE);
    widgets::caption(ui, t, COPYRIGHT, false);
    ui.add_space(layout::SPACE[3]);
    if widgets::button(ui, t, Look::Outline, &HelpItem::Licenses.label(), true).clicked() {
        open(app, Sheet::Licenses);
    }
}

fn keys(app: &mut App, ui: &mut Ui, t: &Tokens) {
    egui::Grid::new("shortcuts")
        .num_columns(2)
        .spacing([layout::SPACE[5], layout::SPACE[2]])
        .show(ui, |ui| {
            for (keys, what) in shortcuts(app.host.platform) {
                widgets::text(ui, &keys, theme::chrome_strong(size::UI), t.ink);
                widgets::text(ui, what, theme::chrome(size::UI), t.ink_soft);
                ui.end_row();
            }
        });
}

fn licences(app: &mut App, ui: &mut Ui, t: &Tokens) {
    widgets::prose(ui, t, LICENCE_LINE);
    ui.add_space(layout::SPACE[4]);
    let row = ui.horizontal_wrapped(|ui| {
        for (i, notice) in NOTICES.iter().enumerate() {
            let selected = app.help.notice == i;
            let r = ui.selectable_label(selected, notice.title);
            ui.ctx().accesskit_node_builder(r.id, |n| {
                n.set_role(accesskit::Role::Tab);
                n.set_selected(selected);
            });
            if r.clicked() {
                app.help.notice = i;
            }
        }
    });
    ui.ctx().accesskit_node_builder(row.response.id, |n| {
        n.set_role(accesskit::Role::TabList);
        n.set_label(HelpItem::Licenses.label());
    });
    ui.add_space(layout::SPACE[4]);
    let notice = NOTICES[app.help.notice.min(NOTICES.len() - 1)];
    let mut text = notice.text;
    let r = ui.add(
        egui::TextEdit::multiline(&mut text)
            .id_salt(("notice", notice.title))
            .font(theme::chrome(size::LABEL))
            .desired_width(f32::INFINITY)
            .desired_rows(24),
    );
    ui.ctx().accesskit_node_builder(r.id, |n| {
        n.set_label(notice.title);
        n.set_read_only();
    });
}

fn updates(app: &mut App, ui: &mut Ui, t: &Tokens) {
    widgets::prose(ui, t, &format!("This is {PRODUCT} {}.", RELEASE.version));
    if !frame::updates_block(app, ui, t)
        && widgets::button(ui, t, Look::Outline, "Open the releases page", true).clicked()
    {
        app.host.open(RELEASES);
    }
}
