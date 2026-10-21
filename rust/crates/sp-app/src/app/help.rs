//! The Help menu's half in the window: what each item does
//! ([`choose`]), the sheets it opens (About, Keyboard Shortcuts, Licenses
//! and Notices, Check for Updates…, and the problem report that a failed
//! conversion's Report a bug opens too), and on Windows and Linux the menu
//! itself, at the foot of the page beside the version stamp ([`menu`]).
//! On a Mac the menu is the menu bar's (menu.rs), which hands its choices
//! here. What the menu holds is model/help.rs.

use egui::accesskit;
use egui::{Id, Sense, Ui, Vec2, WidgetInfo, WidgetType};

use super::widgets::{self, Look};
use super::{App, Purpose, frame};
use crate::engine::{Reply, Request};
use crate::model::convert::Platform;
use crate::model::feedback::{REPORT_ADDRESS, ReportDraft, Shape, os_label};
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
    /// Report a problem: by email, or as a GitHub issue.
    Report,
}

impl Sheet {
    /// The sheet's name, read out as the dialog's.
    pub fn title(self) -> String {
        match self {
            Sheet::About => HelpItem::About.label(),
            Sheet::Shortcuts => HelpItem::Shortcuts.label(),
            Sheet::Licenses => HelpItem::Licenses.label(),
            Sheet::Updates => "Updates".into(),
            Sheet::Report => HelpItem::ReportBug.label(),
        }
    }
}

#[derive(Debug, Default)]
pub struct State {
    /// The sheet open, if one is.
    pub sheet: Option<Sheet>,
    /// Which licence or notice the Licenses sheet shows.
    pub notice: usize,
    /// The report sheet's draft, while it is open.
    pub report: ReportDraft,
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
        HelpItem::OpenSample => {
            app.help.sheet = None;
            app.notes_open = false;
            super::sample::open(app);
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
        HelpItem::ReportBug => open_report(app, None),
        HelpItem::Licenses => open(app, Sheet::Licenses),
        HelpItem::Suite => {
            app.host.open(SUITE);
        }
    }
}

/// Open the report sheet: from Help (`None`), or from a failed
/// conversion with its code and message. For a converted script the
/// engine is asked for its shape, which the sheet offers as a box, off.
pub fn open_report(app: &mut App, refusal: Option<(&str, &str)>) {
    use super::convert::View;
    let fountain = app
        .script
        .as_ref()
        .and_then(|s| s.fountain_path.clone())
        .filter(|_| refusal.is_none() && app.convert.view != View::Failed);
    app.help.report = ReportDraft::new(refusal, app.convert.known_home.as_deref());
    open(app, Sheet::Report);
    if let Some(fountain) = fountain {
        app.help.report.asked = Some(fountain.clone());
        app.call(
            Request::Shape {
                fountain: fountain.clone(),
            },
            Purpose::Shape { fountain },
        );
    }
}

/// The engine counted the script: the sheet can offer the box, if it is
/// still the sheet that asked.
pub fn shape_answered(app: &mut App, fountain: &str, reply: &Reply) {
    if app.help.report.asked.as_deref() != Some(fountain) {
        return;
    }
    app.help.report.shape = reply.answer.as_ref().ok().and_then(Shape::from_answer);
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
            let rem = if sheet == Sheet::Report { 34.0 } else { 44.0 };
            let width = (rem * 16.0f32).min(ctx.content_rect().width() * 0.88);
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
                Sheet::Report => HelpItem::ReportBug.label(),
            };
            let close = frame::sheet_head(ui, &t, &head);
            // The report keeps its long part (what is sent) in a scroll of
            // its own, so its buttons are never scrolled away.
            if sheet == Sheet::Report {
                return report(app, ui, &t) || close;
            }
            let done = egui::ScrollArea::vertical()
                .id_salt("help-sheet-scroll")
                .max_height(ctx.content_rect().height() * 0.82 - 80.0)
                .show(ui, |ui| match sheet {
                    Sheet::About => {
                        about(app, ui, &t);
                        false
                    }
                    Sheet::Shortcuts => {
                        keys(app, ui, &t);
                        false
                    }
                    Sheet::Licenses => {
                        licences(app, ui, &t);
                        false
                    }
                    Sheet::Updates => {
                        updates(app, ui, &t);
                        false
                    }
                    Sheet::Report => unreachable!("drawn above"),
                });
            close || done.inner
        });
    if modal.inner || modal.should_close() {
        // A button inside may have opened another sheet; only this one goes.
        if app.help.sheet == Some(sheet) {
            app.help.sheet = None;
        }
        app.restore_focus = true;
    }
}

/// The report sheet's words (also read by the tests).
pub mod words {
    pub const INTRO: &str = "Say what went wrong. It goes to the people who make Method Reader. No words from your script are sent.";
    pub const WHAT_HAPPENED: &str = "What happened?";
    pub const ADD_SHAPE: &str = "Add counts of the script's shape (no words from it)";
    pub const SHOW_SENT: &str = "Show what's sent";
    pub const HIDE_SENT: &str = "Hide what's sent";
    pub const SENT: &str = "What's sent";
    pub const SEND: &str = "Send by email";
    pub const COPY: &str = "Copy the address";
    pub const GITHUB: &str = "or open an issue on GitHub";
    pub const CANCEL: &str = "Cancel";
    pub const NO_MAIL: &str =
        "No mail app opened. Copy the address and write from any email account.";
}

/// The sheet: the text box, the counts box for a converted script, what is
/// sent, and the ways to send it. What it sends is the model's
/// (model/feedback.rs); this draws it. True when Cancel was chosen or a
/// report went.
fn report(app: &mut App, ui: &mut Ui, t: &Tokens) -> bool {
    let mut done = false;
    widgets::prose(ui, t, words::INTRO);
    ui.add_space(layout::SPACE[3]);
    let label = widgets::text(
        ui,
        words::WHAT_HAPPENED,
        theme::chrome_strong(size::LABEL),
        t.ink,
    );
    let typed = ui.add(
        egui::TextEdit::multiline(&mut app.help.report.typed)
            .id_salt("report-typed")
            .char_limit(crate::model::feedback::MAX_TYPED)
            .font(theme::chrome(size::UI))
            .desired_width(f32::INFINITY)
            .desired_rows(5),
    );
    ui.ctx().accesskit_node_builder(typed.id, |n| {
        n.set_label(words::WHAT_HAPPENED);
        n.push_labelled_by(label.id.accesskit_id());
    });
    if app.help.report.shape.is_some() {
        ui.add_space(layout::SPACE[2]);
        ui.add(egui::Checkbox::new(
            &mut app.help.report.add_shape,
            egui::RichText::new(words::ADD_SHAPE)
                .font(theme::chrome(size::UI))
                .color(t.ink),
        ));
    }
    let sent = app
        .help
        .report
        .report(RELEASE.version, &os_label(app.host.platform.word()));
    ui.add_space(layout::SPACE[2]);
    let open = app.help.report.show_sent;
    let toggle = widgets::button(
        ui,
        t,
        Look::Quiet,
        if open {
            words::HIDE_SENT
        } else {
            words::SHOW_SENT
        },
        true,
    );
    ui.ctx()
        .accesskit_node_builder(toggle.id, |n| n.set_expanded(open));
    if toggle.clicked() {
        app.help.report.show_sent = !open;
    }
    if app.help.report.show_sent {
        let mut body = format!(
            "To: {REPORT_ADDRESS}\nSubject: {}\n\n{}",
            sent.subject(),
            sent.body()
        );
        // Its own scroll, so the buttons below stay in view.
        egui::ScrollArea::vertical()
            .id_salt("report-sent-scroll")
            .max_height(10.0 * size::LABEL * 1.4)
            .show(ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::multiline(&mut body)
                        .id_salt("report-sent")
                        .font(theme::chrome(size::LABEL))
                        .desired_width(f32::INFINITY)
                        .desired_rows(6),
                );
                ui.ctx().accesskit_node_builder(r.id, |n| {
                    n.set_label(words::SENT);
                    n.set_read_only();
                });
            });
    }
    ui.add_space(layout::SPACE[3]);
    ui.horizontal_wrapped(|ui| {
        if widgets::button(ui, t, Look::Brass, words::SEND, sent.can_send()).clicked() {
            if app.host.open(&sent.mailto_url()) {
                done = true;
            } else {
                app.help.report.mail_failed = true;
            }
        }
        if widgets::button(ui, t, Look::Outline, words::COPY, true).clicked() {
            ui.ctx().copy_text(REPORT_ADDRESS.to_string());
            app.help.report.copied = true;
        }
        if widgets::button(ui, t, Look::Quiet, words::CANCEL, true).clicked() {
            done = true;
        }
    });
    let said = if app.help.report.copied {
        format!("Copied {REPORT_ADDRESS}.")
    } else if app.help.report.mail_failed {
        words::NO_MAIL.to_string()
    } else {
        format!("The address is {REPORT_ADDRESS}.")
    };
    widgets::live(
        ui,
        Id::new("report-said"),
        &said,
        theme::chrome(size::LABEL),
        if app.help.report.mail_failed && !app.help.report.copied {
            t.alarm
        } else {
            t.ink_muted
        },
    );
    ui.add_space(layout::SPACE[2]);
    let link = ui.link(words::GITHUB);
    ui.ctx()
        .accesskit_node_builder(link.id, |n| n.set_role(accesskit::Role::Link));
    if link.clicked() {
        app.host.open(&sent.issue_url());
        done = true;
    }
    done
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
