//! The send panel: the right half of Convert's result
//! (`desktop/ui/send-panel.js` and `kfx.js`, below their lines). The cards,
//! the calm screen with its one brass button, Other ways, My device isn't
//! here, the asterisk and its reports, Eject, the Kindle file built ahead,
//! and the KFX checklist. Every rule is device.js's, send-routes.js's or
//! kfx.js's, ported in src/model.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Id, Rect, Sense, Ui, Vec2, WidgetInfo, WidgetType, accesskit};
use serde_json::{Value, json};

use super::widgets::{self, Look};
use super::{App, Purpose as AppPurpose, SendResult, convert};
use crate::engine::{Reply, Request};
use crate::model::device::{
    self as d, Eject, WayItem, cards_for, copied_state, eject_is_beta, eject_label, eject_outcome,
    eject_still_there, fold_unplugged, kindle_session, offer_for, save_rows, save_turn,
    send_it_rows,
};
use crate::model::json::ok;
use crate::model::kfx::{
    self as k, Checklist, Control, after_install, control_for, setup_from, show_setup,
};
use crate::model::layout;
use crate::model::notes::RELEASE;
use crate::model::requests::{self as q, answer_from, device_form, report_form, report_route};
use crate::model::send_routes::{
    self as r, Device, KindleFile, Performer, Route, Routes, Status, attempt_key, blocked_reason,
    build_line, builds_ahead, builds_on_press, button_label, copying, email_setup_hint,
    failure_message, for_format, kindle_check_from, kindle_file_is_beta, needs_settings,
    options_json_for, performer_for, preparing, reads_the_book, route_lines, route_note_from,
    routes_failure, routes_from, same_routes, save_filter_for, save_name_for,
};
use crate::model::tune::settings_from;
use crate::theme::{self, size};

/// How often the calm screen for a reMarkable asks the full list again.
pub const REMARKABLE_LOOK_MS: f64 = 10_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Cards,
    Calm,
    Missing,
}

#[derive(Clone, Debug)]
pub enum Flow {
    Device(Device),
    Save(&'static str),
}

#[derive(Clone, Debug, Default)]
pub struct Build {
    pub runs: u32,
    pub extension: Option<String>,
    pub file: Option<KindleFile>,
}

#[derive(Default)]
pub struct RequestDraft {
    pub draft: String,
    pub busy: bool,
    pub said: Option<(String, bool)>,
}

#[derive(Default, Clone)]
pub struct Report {
    pub busy: bool,
    pub last: Option<bool>,
    pub said: Option<(String, bool)>,
}

#[derive(Default)]
pub struct Kfx {
    pub setup: Option<Checklist>,
    pub just_installed: bool,
    pub installing: bool,
    probing: bool,
    installs: u64,
    pub status: Status,
}

enum Net {
    Device((bool, String)),
    Report(String, (bool, String)),
}

pub struct State {
    pub mode: Option<Mode>,
    pub blocked: Option<&'static str>,
    pub mounted_book: Option<String>,
    mounts: u64,
    pub drawn: Option<Routes>,
    pub routes_fault: Option<String>,
    asked: bool,
    owed: bool,
    asks: u64,
    newest: u64,
    pub kindle: Option<KindleFile>,
    kindle_book: Option<String>,
    labelled_with: Option<String>,
    kindle_lines: HashMap<String, VecDeque<(Request, AppPurpose)>>,
    kindle_busy: HashSet<String>,
    pub builds: HashMap<String, Build>,
    tried_for: HashMap<String, String>,
    ahead_wanted: bool,
    pub showing: bool,
    pub sending: bool,
    pub era: u64,
    pub ways_open: bool,
    pub kfx_open: bool,
    pub status: Status,
    pub artifact: String,
    pub request: RequestDraft,
    pub pinned: HashSet<String>,
    pub reports: HashMap<String, Report>,
    remarkable_next: Option<f64>,
    pub kfx: Kfx,
    net_tx: Sender<Net>,
    net_rx: Receiver<Net>,
}

impl Default for State {
    fn default() -> Self {
        let (net_tx, net_rx) = channel();
        Self {
            mode: None,
            blocked: None,
            mounted_book: None,
            mounts: 0,
            drawn: None,
            routes_fault: None,
            asked: false,
            owed: false,
            asks: 0,
            newest: 0,
            kindle: None,
            kindle_book: None,
            labelled_with: None,
            kindle_lines: HashMap::new(),
            kindle_busy: HashSet::new(),
            builds: HashMap::new(),
            tried_for: HashMap::new(),
            ahead_wanted: false,
            showing: false,
            sending: false,
            era: 0,
            ways_open: false,
            kfx_open: false,
            status: Status::default(),
            artifact: String::new(),
            request: RequestDraft::default(),
            pinned: HashSet::new(),
            reports: HashMap::new(),
            remarkable_next: None,
            kfx: Kfx::default(),
            net_tx,
            net_rx,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Purpose {
    ReaderDevice {
        mount: u64,
    },
    StoreDevice,
    Routes {
        era: u64,
        ask: u64,
    },
    KindleCheck {
        era: u64,
        ahead: bool,
    },
    BuildAhead {
        book: String,
    },
    Settings {
        era: u64,
        next: Flow,
    },
    Export {
        era: u64,
        device: Device,
        building: bool,
        book: String,
    },
    Send {
        era: u64,
        device: Device,
        built: Value,
        title: String,
    },
    Perform {
        era: u64,
    },
    Save {
        era: u64,
        kind: &'static str,
        book: String,
        tracked: bool,
    },
    Eject {
        era: u64,
        eject: Eject,
    },
    KfxProbe {
        installs: u64,
    },
    KfxInstall,
}

fn p(purpose: Purpose) -> AppPurpose {
    AppPurpose::Panel(purpose)
}

fn book(app: &App) -> Option<String> {
    app.script.as_ref().and_then(|s| s.epub_path.clone())
}

// ------------------------------------------------------------------ mount

/// A fresh result: the panel for the book on screen.
pub fn mount(app: &mut App) {
    let b = book(app);
    let s = &mut app.panel;
    s.mounts += 1;
    s.mode = None;
    s.ways_open = false;
    s.kfx_open = false;
    if b != s.mounted_book {
        s.era += 1;
        s.mounted_book = b.clone();
        s.drawn = None;
        s.routes_fault = None;
        s.ahead_wanted = false;
    }
    if b != s.kindle_book {
        s.kindle = None;
        s.kindle_book = None;
    }
    s.asked = false;
    s.artifact.clear();
    // What the last send said about this book comes back; another book's
    // line does not, but a reader still waiting to be ejected does.
    match app.send_result.clone() {
        Some(kept) if kept.book == b => {
            app.panel.status = Status {
                line: kept.line,
                bad: kept.bad,
            }
        }
        Some(SendResult {
            eject: Some(eject), ..
        }) => {
            app.send_result = Some(SendResult {
                book: b.clone(),
                line: eject.line.clone(),
                bad: false,
                eject: Some(eject.clone()),
            });
            app.panel.status = Status::good(eject.line);
        }
        _ => {
            app.send_result = None;
            app.panel.status = Status::default();
        }
    }
    app.panel.blocked = blocked_reason(app.script.as_ref());
    if app.panel.blocked.is_some() {
        return;
    }
    match app.reader_device {
        None => {
            let mount = app.panel.mounts;
            app.call(Request::AppSettings, p(Purpose::ReaderDevice { mount }));
        }
        Some(device) => set_mode(
            app,
            if device.is_none() {
                Mode::Cards
            } else {
                Mode::Calm
            },
        ),
    }
}

fn set_mode(app: &mut App, mode: Mode) {
    app.panel.mode = Some(mode);
    if mode == Mode::Calm && !app.panel.asked {
        app.panel.asked = true;
        refresh(app, true);
        refresh(app, false);
    }
}

pub fn shown(app: &mut App) {
    app.panel.showing = true;
    if app.panel.mode == Some(Mode::Calm) && !app.panel.asked {
        set_mode(app, Mode::Calm);
    }
    if let Some(file) = app.panel.kindle.as_mut() {
        file.fresh = None;
    }
    app.panel.ahead_wanted = true;
    check_kindle(app, true);
    if app.panel.kfx_open {
        kfx_probe(app);
    }
}

pub fn hidden(app: &mut App) {
    app.panel.showing = false;
    app.panel.ahead_wanted = false;
}

/// The heads-up's Copy: the same flow as a card's brass button.
pub fn copy_to(app: &mut App, device: Device) {
    if blocked_reason(app.script.as_ref()).is_some() || app.convert.view != convert::View::Done {
        return;
    }
    send_to(app, device);
}

/// The window got the focus back: name the Kindle file again (no build),
/// ask the KFX checklist again, look for a reMarkable.
pub fn focus_returned(app: &mut App) {
    if !app.panel.showing {
        return;
    }
    check_kindle(app, false);
    if app.panel.kfx_open {
        kfx_probe(app);
    }
    if remarkable_wanted(app) && !app.panel.sending {
        refresh(app, false);
    }
}

fn remarkable_wanted(app: &App) -> bool {
    app.panel.showing
        && app.panel.mode == Some(Mode::Calm)
        && app.reader_device == Some(Some("remarkable"))
}

pub fn tick(app: &mut App, now: f64) {
    if remarkable_wanted(app) {
        match app.panel.remarkable_next {
            None => app.panel.remarkable_next = Some(now + REMARKABLE_LOOK_MS),
            Some(at) if now >= at => {
                app.panel.remarkable_next = Some(now + REMARKABLE_LOOK_MS);
                if !app.panel.sending {
                    refresh(app, false);
                }
            }
            _ => {}
        }
    } else {
        app.panel.remarkable_next = None;
    }
    while let Ok(net) = app.panel.net_rx.try_recv() {
        match net {
            Net::Device((ok, line)) => {
                let req = &mut app.panel.request;
                req.busy = false;
                if ok {
                    req.draft.clear();
                }
                req.said = Some((line, !ok));
            }
            Net::Report(kind, (ok, line)) => {
                let rep = app.panel.reports.entry(kind).or_default();
                rep.busy = false;
                rep.said = Some((line, !ok));
            }
        }
    }
}

// ----------------------------------------------------------------- routes

fn refresh(app: &mut App, quick: bool) {
    let Some(epub) = book(app) else { return };
    if app.panel.sending {
        app.panel.owed = true;
        return;
    }
    app.panel.asks += 1;
    let (era, ask) = (app.panel.era, app.panel.asks);
    app.call(
        Request::Routes { epub, quick },
        p(Purpose::Routes { era, ask }),
    );
}

fn fault(app: &mut App, line: String) {
    app.panel.drawn = None;
    app.panel.routes_fault = Some(line);
}

pub fn devices_changed(app: &mut App, list: &[Device]) {
    if let Some(eject) = app.send_result.as_ref().and_then(|s| s.eject.clone())
        && !eject_still_there(&eject, Some(list))
        && let Some(result) = app.send_result.as_mut()
    {
        result.eject = None;
    }
    if app.panel.sending {
        app.panel.owed = true;
        return;
    }
    maybe_build_ahead(app);
    match app.panel.mode {
        Some(Mode::Cards) => app.panel.asked = false,
        Some(Mode::Calm) if app.panel.showing => {
            refresh(app, true);
            refresh(app, false);
        }
        Some(Mode::Calm) => app.panel.asked = false,
        _ => {}
    }
}

// ----------------------------------------------------------- Kindle file

fn check_kindle(app: &mut App, ahead: bool) {
    if blocked_reason(app.script.as_ref()).is_some() {
        return;
    }
    let Some(b) = book(app) else { return };
    if let Some(running) = app.panel.builds.get(&b) {
        if let Some(file) = running.file.clone() {
            app.panel.kindle = Some(file);
            app.panel.kindle_book = Some(b);
        }
        return;
    }
    let era = app.panel.era;
    app.on_book(
        &b.clone(),
        "check",
        Request::KindleCheck { epub: b },
        p(Purpose::KindleCheck { era, ahead }),
    );
}

fn maybe_build_ahead(app: &mut App) {
    if !app.panel.ahead_wanted || !app.panel.showing {
        return;
    }
    let Some(b) = book(app) else { return };
    if app.panel.kindle_book.as_deref() != Some(b.as_str()) {
        return;
    }
    let for_kindle = kindle_session(app.reader_device.flatten(), app.devices.as_deref());
    let busy = app.panel.sending || app.panel.kfx.installing;
    if !builds_ahead(
        app.panel.kindle.as_ref(),
        app.panel.builds.contains_key(&b),
        busy,
        app.panel.tried_for.get(&b).map(String::as_str),
        for_kindle,
    ) {
        return;
    }
    let file = app.panel.kindle.clone().expect("builds_ahead needs a file");
    app.panel.tried_for.insert(b.clone(), attempt_key(&file));
    let script = app.script.clone().expect("a book");
    track(app, &b, Some(&file), Some(file.extension.clone()));
    let request = Request::Export {
        epub: b.clone(),
        for_format: "kindle".into(),
        fountain: script.fountain_path.clone(),
        options: options_json_for(&script),
        out: None,
    };
    on_kindle_file(app, &b, request, p(Purpose::BuildAhead { book: b.clone() }));
}

fn track(app: &mut App, book: &str, file: Option<&KindleFile>, extension: Option<String>) {
    let entry = app.panel.builds.entry(book.into()).or_default();
    entry.runs += 1;
    if let Some(f) = file {
        entry.file = Some(f.clone());
    }
    if extension.is_some() {
        entry.extension = extension;
    }
}

fn untrack(app: &mut App, book: &str) {
    if let Some(entry) = app.panel.builds.get_mut(book) {
        entry.runs = entry.runs.saturating_sub(1);
        if entry.runs == 0 {
            app.panel.builds.remove(book);
        }
    }
}

/// Kindle-file exports of one book, one at a time, before their book turn.
fn on_kindle_file(app: &mut App, book: &str, request: Request, purpose: AppPurpose) {
    if app.panel.kindle_busy.contains(book) {
        app.panel
            .kindle_lines
            .entry(book.into())
            .or_default()
            .push_back((request, purpose));
        return;
    }
    app.panel.kindle_busy.insert(book.into());
    app.on_book(book, "kindle", request, purpose);
}

fn kindle_line_done(app: &mut App, book: &str) {
    if let Some((request, purpose)) = app
        .panel
        .kindle_lines
        .get_mut(book)
        .and_then(VecDeque::pop_front)
    {
        app.on_book(book, "kindle", request, purpose);
    } else {
        app.panel.kindle_busy.remove(book);
    }
}

fn built_now(app: &mut App, book: &str) {
    if app.panel.kindle_book.as_deref() == Some(book)
        && let Some(file) = app.panel.kindle.as_mut()
    {
        file.fresh = Some(true);
    }
}

// ------------------------------------------------------------------ flows

fn say(app: &mut App, status: Status, eject: Option<Option<Eject>>) {
    let kept = app.send_result.as_ref().and_then(|s| s.eject.clone());
    app.send_result = Some(SendResult {
        book: app.panel.mounted_book.clone(),
        line: status.line.clone(),
        bad: status.bad,
        eject: eject.unwrap_or(kept),
    });
    app.panel.status = status;
}

fn begin(app: &mut App) -> bool {
    if app.panel.sending || app.panel.kfx.installing {
        return false;
    }
    app.panel.sending = true;
    app.panel.artifact.clear();
    true
}

fn finish(app: &mut App, worked: bool, era: u64) {
    app.panel.sending = false;
    let stale = era != app.panel.era;
    if app.panel.owed {
        app.panel.owed = false;
        app.panel.newest = app.panel.asks;
        refresh(app, true);
        refresh(app, false);
    } else if worked && !stale {
        app.panel.newest = app.panel.asks;
        refresh(app, false);
    }
    if !stale {
        maybe_build_ahead(app);
    }
    app.restore_focus = true;
}

fn ensure_settings(app: &mut App, next: Flow) {
    if needs_settings(app.script.as_ref()) {
        let fountain = app
            .script
            .as_ref()
            .and_then(|s| s.fountain_path.clone())
            .unwrap_or_default();
        let era = app.panel.era;
        app.call(
            Request::Settings {
                fountain,
                set: None,
            },
            p(Purpose::Settings { era, next }),
        );
    } else {
        run_flow(app, next);
    }
}

fn run_flow(app: &mut App, flow: Flow) {
    match flow {
        Flow::Device(device) => export_for(app, device),
        Flow::Save(kind) => save_dialog_then_export(app, kind),
    }
}

fn send_to(app: &mut App, device: Device) {
    if !begin(app) {
        return;
    }
    ensure_settings(app, Flow::Device(device));
}

fn export_for(app: &mut App, device: Device) {
    let Some(script) = app.script.clone() else {
        return;
    };
    let Some(b) = script.epub_path.clone() else {
        return;
    };
    let building = builds_on_press(&device, app.panel.kindle.as_ref());
    say(
        app,
        if building {
            Status::default()
        } else {
            preparing(&device)
        },
        None,
    );
    let request = Request::Export {
        epub: b.clone(),
        for_format: for_format(&device).into(),
        fountain: script.fountain_path.clone(),
        options: options_json_for(&script),
        out: None,
    };
    let era = app.panel.era;
    let purpose = p(Purpose::Export {
        era,
        device: device.clone(),
        building,
        book: b.clone(),
    });
    if for_format(&device) == "kindle" {
        if building {
            let file = app.panel.kindle.clone();
            let ext = file.as_ref().map(|f| f.extension.clone());
            track(app, &b, file.as_ref(), ext);
        }
        on_kindle_file(app, &b, request, purpose);
    } else {
        app.on_book(&b, "send", request, purpose);
    }
}

fn choose_route(app: &mut App, route: &Route) {
    match performer_for(route) {
        Performer::Device => {
            if let Some(device) = route.device.clone() {
                send_to(app, device);
            }
        }
        Performer::SaveEpub => save_file(app, "epub"),
        _ => perform(
            app,
            route.key.clone(),
            route.title.clone(),
            performer_for(route),
        ),
    }
}

fn perform(app: &mut App, key: String, title: String, how: Performer) {
    if !begin(app) {
        return;
    }
    let Some(b) = book(app) else { return };
    say(app, r::opening(&title), None);
    let request = if how == Performer::Setup {
        Request::EmailSetup
    } else {
        Request::Route {
            key,
            epub: b.clone(),
            out: None,
            fountain: None,
            options: None,
        }
    };
    let era = app.panel.era;
    if reads_the_book(how) {
        app.on_book(&b, "copy", request, p(Purpose::Perform { era }));
    } else {
        app.call(request, p(Purpose::Perform { era }));
    }
}

fn save_file(app: &mut App, kind: &'static str) {
    if !begin(app) {
        return;
    }
    ensure_settings(app, Flow::Save(kind));
}

fn save_dialog_then_export(app: &mut App, kind: &'static str) {
    let Some(script) = app.script.clone() else {
        return;
    };
    let Some(b) = script.epub_path.clone() else {
        return;
    };
    let era = app.panel.era;
    let label = d::SAVE_TYPES
        .iter()
        .find(|t| t.kind == kind)
        .map(|t| t.label);
    if !app.dialogs.try_open(&mut app.busy) {
        finish(app, false, era);
        return;
    }
    let out = app
        .host
        .dialogs
        .save(&save_name_for(&b, kind), &save_filter_for(kind, label));
    let now = app.now();
    app.dialogs.closed(&mut app.busy, now);
    let Some(out) = out else {
        finish(app, false, era);
        return;
    };
    let request = Request::Export {
        epub: b.clone(),
        for_format: kind.into(),
        fountain: script.fountain_path.clone(),
        options: options_json_for(&script),
        out: Some(out),
    };
    if save_turn(kind) == "kindle" {
        say(app, Status::default(), None);
        let same = app.panel.kindle_book.as_deref() == Some(b.as_str())
            && app
                .panel
                .kindle
                .as_ref()
                .is_some_and(|f| f.extension == kind);
        let tracked = !same || r::may_build(app.panel.kindle.as_ref());
        if tracked {
            let file = if same { app.panel.kindle.clone() } else { None };
            track(app, &b, file.as_ref(), Some(kind.into()));
        }
        on_kindle_file(
            app,
            &b,
            request,
            p(Purpose::Save {
                era,
                kind,
                book: b.clone(),
                tracked,
            }),
        );
    } else {
        say(app, Status::good(r::SAVING), None);
        app.on_book(
            &b,
            "copy",
            request,
            p(Purpose::Save {
                era,
                kind,
                book: b.clone(),
                tracked: false,
            }),
        );
    }
}

fn eject_now(app: &mut App) {
    let Some(eject) = app.send_result.as_ref().and_then(|s| s.eject.clone()) else {
        return;
    };
    if !begin(app) {
        return;
    }
    say(app, Status::good(format!("Ejecting {}…", eject.name)), None);
    let era = app.panel.era;
    app.call(
        Request::Eject {
            volume: eject.volume.clone(),
        },
        p(Purpose::Eject { era, eject }),
    );
}

fn choose_device(app: &mut App, kind: &'static str) {
    app.reader_device = Some(Some(kind));
    app.panel.asked = false;
    app.panel.drawn = None;
    set_mode(app, Mode::Calm);
    say(app, Status::default(), None);
    app.call(
        Request::SetAppSettings {
            json: json!({ "readerDevice": kind }).to_string(),
        },
        p(Purpose::StoreDevice),
    );
    app.restore_focus = true;
}

fn send_device_request(app: &mut App) {
    let req = &mut app.panel.request;
    if req.busy || req.draft.trim().is_empty() {
        return;
    }
    req.busy = true;
    let form = device_form(&req.draft, RELEASE.version, app.host.platform);
    let poster = app.host.poster.clone();
    let tx = app.panel.net_tx.clone();
    std::thread::spawn(move || {
        let answer = answer_from(poster.post(q::REQUEST_URL, &form));
        let _ = tx.send(Net::Device(answer));
    });
}

fn send_report(app: &mut App, kind: &str, worked: bool) {
    let rep = app.panel.reports.entry(kind.into()).or_default();
    if rep.busy {
        return;
    }
    rep.busy = true;
    rep.last = Some(worked);
    let form = report_form(kind, worked, RELEASE.version, app.host.platform);
    let poster = app.host.poster.clone();
    let tx = app.panel.net_tx.clone();
    let kind = kind.to_string();
    std::thread::spawn(move || {
        let answer = answer_from(poster.post(q::REQUEST_URL, &form));
        let _ = tx.send(Net::Report(kind, answer));
    });
}

// ------------------------------------------------------------------- KFX

fn open_kfx(app: &mut App) {
    if app.panel.kfx_open {
        return;
    }
    app.panel.kfx_open = true;
    if !app.panel.kfx.installing {
        app.panel.kfx.just_installed = false;
        app.panel.kfx.status = Status::default();
    }
    kfx_probe(app);
}

fn kfx_probe(app: &mut App) {
    if app.panel.kfx.probing || app.panel.kfx.installing {
        return;
    }
    app.panel.kfx.probing = true;
    let installs = app.panel.kfx.installs;
    app.call(Request::KfxStatus, p(Purpose::KfxProbe { installs }));
}

fn kfx_install(app: &mut App) {
    if app.panel.kfx.installing || app.panel.sending {
        return;
    }
    app.panel.kfx.installing = true;
    app.panel.kfx.installs += 1;
    app.panel.kfx.status = Status::good(k::INSTALLING);
    app.call(Request::KfxInstall, p(Purpose::KfxInstall));
}

// --------------------------------------------------------------- answers

pub fn answered(app: &mut App, purpose: Purpose, reply: Reply) {
    let answer = App::answer_or(&reply);
    match purpose {
        Purpose::ReaderDevice { mount } => {
            if mount != app.panel.mounts {
                return;
            }
            let device = reply.answer.ok().as_ref().and_then(d::reader_device_from);
            app.reader_device = Some(device);
            set_mode(
                app,
                if device.is_none() {
                    Mode::Cards
                } else {
                    Mode::Calm
                },
            );
            app.restore_focus = true;
        }
        Purpose::StoreDevice => match reply.answer {
            Ok(a) if ok(&a) => {}
            Ok(a) => say(app, Status::failed(&failure_message(&a)), None),
            Err(m) => say(app, Status::failed(&m), None),
        },
        Purpose::Routes { era, ask } => {
            if era != app.panel.era || app.panel.sending || ask <= app.panel.newest {
                if era == app.panel.era && app.panel.sending {
                    app.panel.owed = true;
                }
                return;
            }
            app.panel.newest = ask;
            let answer = match reply.answer {
                Ok(a) => a,
                Err(m) => return fault(app, m),
            };
            let Some(shown) = routes_from(&answer) else {
                return fault(app, routes_failure(&answer));
            };
            let label = app.panel.kindle.as_ref().map(|k| k.extension.clone());
            if app.panel.routes_fault.is_none()
                && same_routes(app.panel.drawn.as_ref(), Some(&shown))
                && app.panel.labelled_with == label
            {
                return;
            }
            app.panel.drawn = Some(shown);
            app.panel.routes_fault = None;
            maybe_build_ahead(app);
        }
        Purpose::KindleCheck { era, ahead } => {
            if era != app.panel.era {
                return;
            }
            app.panel.kindle = kindle_check_from(&answer);
            app.panel.kindle_book = book(app);
            if ahead {
                maybe_build_ahead(app);
            }
        }
        Purpose::BuildAhead { book: b } => {
            untrack(app, &b);
            kindle_line_done(app, &b);
            if ok(&answer) && app.panel.showing {
                check_kindle(app, false);
            }
        }
        Purpose::Settings { era, next } => {
            if era != app.panel.era {
                return finish(app, false, era);
            }
            if let Some(script) = app.script.as_mut() {
                script.settings = settings_from(&answer);
            }
            run_flow(app, next);
        }
        Purpose::Export {
            era,
            device,
            building,
            book: b,
        } => {
            if building {
                untrack(app, &b);
            }
            if for_format(&device) == "kindle" {
                kindle_line_done(app, &b);
            }
            if era != app.panel.era {
                return finish(app, false, era);
            }
            if let Err(message) = &reply.answer {
                say(app, Status::failed(message), None);
                return finish(app, false, era);
            }
            if !ok(&answer) {
                say(app, Status::failed(&failure_message(&answer)), None);
                return finish(app, false, era);
            }
            if building {
                built_now(app, &b);
            }
            say(app, copying(&device), None);
            let file = answer
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let title = app
                .script
                .as_ref()
                .map(|s| s.title.clone())
                .unwrap_or_default();
            app.on_book(
                &b,
                "send",
                Request::Send {
                    file,
                    device: Some(device.id.clone()),
                },
                p(Purpose::Send {
                    era,
                    device,
                    built: answer,
                    title,
                }),
            );
        }
        Purpose::Send {
            era,
            device,
            built,
            title,
        } => {
            if era != app.panel.era {
                // Another book came up meanwhile: its line is not this one's,
                // but a reader to eject comes along.
                if let Ok(sent) = &reply.answer
                    && r::outcome_for(&built, Some(sent), &device).is_ok()
                    && let (line, Some(mut eject)) = copied_state(&title, &device, sent)
                {
                    eject.line = line.clone();
                    let mounted = app.panel.mounted_book.clone();
                    app.send_result = Some(match app.send_result.clone() {
                        Some(now) if now.book == mounted && !now.line.is_empty() => SendResult {
                            eject: Some(eject),
                            ..now
                        },
                        _ => SendResult {
                            book: mounted,
                            line,
                            bad: false,
                            eject: Some(eject),
                        },
                    });
                }
                return finish(app, false, era);
            }
            let sent = match reply.answer {
                Ok(s) => s,
                Err(m) => {
                    say(app, Status::failed(&m), None);
                    return finish(app, false, era);
                }
            };
            match r::outcome_for(&built, Some(&sent), &device) {
                Err(why) => {
                    say(app, Status::failed(&why), None);
                    finish(app, false, era);
                }
                Ok(_) => {
                    let (line, eject) = copied_state(&title, &device, &sent);
                    let eject = eject.map(|e| Eject {
                        line: line.clone(),
                        ..e
                    });
                    let has_eject = eject.is_some();
                    say(app, Status::good(line), Some(eject));
                    if has_eject && app.panel.mode != Some(Mode::Calm) {
                        if app.reader_device.flatten().is_none()
                            && let Some(kind) = d::kinds().find(|k| *k == device.kind)
                        {
                            app.reader_device = Some(Some(kind));
                            app.call(
                                Request::SetAppSettings {
                                    json: json!({ "readerDevice": kind }).to_string(),
                                },
                                p(Purpose::StoreDevice),
                            );
                        }
                        set_mode(app, Mode::Calm);
                    }
                    app.panel.artifact = r::artifact_line(&built);
                    finish(app, true, era);
                }
            }
        }
        Purpose::Perform { era } => {
            if era == app.panel.era {
                let status = match &reply.answer {
                    Err(m) => Status::failed(m),
                    Ok(a) => match route_note_from(a) {
                        Ok(note) => Status::good(note),
                        Err(why) => Status::failed(&why),
                    },
                };
                let worked = !status.bad;
                say(app, status, None);
                finish(app, worked, era);
            } else {
                finish(app, false, era);
            }
        }
        Purpose::Save {
            era,
            kind,
            book: b,
            tracked,
        } => {
            if tracked {
                untrack(app, &b);
            }
            if save_turn(kind) == "kindle" {
                kindle_line_done(app, &b);
                if ok(&answer)
                    && app
                        .panel
                        .kindle
                        .as_ref()
                        .is_some_and(|f| f.extension == kind)
                {
                    built_now(app, &b);
                }
            }
            if era == app.panel.era {
                let status = match &reply.answer {
                    Err(m) => Status::failed(m),
                    Ok(a) if ok(a) => Status::good(format!(
                        "Saved {}.",
                        r::base_name(a.get("path").and_then(Value::as_str).unwrap_or(""))
                    )),
                    Ok(a) => Status::failed(&failure_message(a)),
                };
                say(app, status, None);
            }
            finish(app, false, era);
        }
        Purpose::Eject { era, eject } => {
            if era == app.panel.era {
                let (line, bad, keep) = match &reply.answer {
                    Ok(a) => eject_outcome(a, &eject),
                    Err(m) => (m.clone(), true, true),
                };
                let still = eject_still_there(&eject, app.devices.as_deref());
                say(
                    app,
                    Status { line, bad },
                    Some(if keep && still { Some(eject) } else { None }),
                );
            }
            finish(app, false, era);
        }
        Purpose::KfxProbe { installs } => {
            app.panel.kfx.probing = false;
            if installs != app.panel.kfx.installs {
                return;
            }
            let next = reply.answer.ok().as_ref().and_then(setup_from);
            if next == app.panel.kfx.setup {
                return;
            }
            app.panel.kfx.setup = next;
            app.panel.kfx.just_installed = false;
            app.panel.kfx.status = Status::default();
        }
        Purpose::KfxInstall => {
            app.panel.kfx.installing = false;
            let after = after_install(&answer, app.panel.kfx.setup.as_ref());
            app.panel.kfx.setup = after.setup;
            app.panel.kfx.just_installed = after.just_installed;
            app.panel.kfx.status = Status {
                line: after.line,
                bad: after.bad,
            };
            check_kindle(app, true);
        }
    }
}

// ---------------------------------------------------------------- drawing

pub fn draw(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    if let Some(blocked) = app.panel.blocked {
        widgets::heading(
            ui,
            &widgets::caps("No book to send"),
            theme::structure_bold(size::BODY),
            t.alarm,
        );
        widgets::text(ui, blocked, theme::structure(size::UI), t.ink);
        ui.add_space(layout::SPACE[6]);
        let b = widgets::button(ui, &t, Look::Outline, "Convert it again", true);
        app.stop(b.id, true);
        if b.clicked() {
            convert::convert_again(app);
        }
        return;
    }
    match app.panel.mode {
        None => {}
        Some(Mode::Cards) => draw_cards(app, ui),
        Some(Mode::Calm) => draw_calm(app, ui),
        Some(Mode::Missing) => draw_missing(app, ui),
    }
    let status = app.panel.status.clone();
    widgets::live(
        ui,
        Id::new("send-status"),
        &status.line,
        theme::prose(size::CAPTION),
        if status.bad { t.alarm } else { t.ink_muted },
    );
    if !app.panel.artifact.is_empty() {
        let artifact = app.panel.artifact.clone();
        widgets::text(ui, &artifact, theme::structure(size::FINE), t.ink_muted);
    }
}

fn enabled(app: &App) -> bool {
    !app.panel.sending && !app.panel.kfx.installing
}

fn plugged_in_mark(ui: &mut Ui, t: &theme::Tokens) {
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, t.present);
        widgets::text(ui, d::PLUGGED_IN, theme::structure(size::NOTE), t.ink_muted);
    });
}

fn draw_cards(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    widgets::heading(
        ui,
        &widgets::caps(d::QUESTION),
        theme::structure_bold(size::UI),
        t.ink,
    );
    ui.add_space(layout::SPACE[4]);
    let cards = cards_for(app.host.platform, app.devices.as_deref());
    let columns = ((ui.available_width() + 12.8) / (176.0 + 12.8))
        .floor()
        .max(1.0) as usize;
    let width = (ui.available_width() - 12.8 * (columns as f32 - 1.0)) / columns as f32;
    for row in cards.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.8;
            for card in row {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 72.0), Sense::click());
                let id = Id::new(("card", card.kind));
                let resp = ui.interact(rect, id, Sense::click()).union(resp);
                let name = if card.here {
                    format!("{}, plugged in", card.name)
                } else {
                    card.name.to_string()
                };
                resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &name));
                let border = if card.here {
                    t.brass
                } else if resp.hovered() {
                    t.ink
                } else {
                    t.ink_muted
                };
                ui.painter().rect_filled(
                    rect,
                    theme::RADIUS_WELL,
                    if resp.hovered() { t.hole } else { t.paper },
                );
                ui.painter().rect_stroke(
                    rect,
                    theme::RADIUS_WELL,
                    egui::Stroke::new(1.0, border),
                    egui::StrokeKind::Inside,
                );
                if card.here {
                    ui.painter().rect_filled(
                        Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
                        0.0,
                        t.brass,
                    );
                }
                let mut y = rect.top() + 14.0;
                let name_galley = ui.painter().layout(
                    card.name.to_string(),
                    theme::structure_bold(size::UI),
                    t.ink,
                    rect.width() - 40.0,
                );
                ui.painter().galley(
                    egui::pos2(rect.left() + 16.0, y),
                    name_galley.clone(),
                    t.ink,
                );
                y += name_galley.size().y + 4.0;
                if let Some(sub) = card.sub {
                    ui.painter().text(
                        egui::pos2(rect.left() + 16.0, y),
                        egui::Align2::LEFT_TOP,
                        sub,
                        theme::structure(size::NOTE),
                        t.ink_muted,
                    );
                }
                if card.here {
                    let at = rect.right_bottom() - Vec2::new(90.0, 18.0);
                    ui.painter()
                        .circle_filled(at + Vec2::new(0.0, 6.0), 4.0, t.present);
                    ui.painter().text(
                        at + Vec2::new(8.0, 0.0),
                        egui::Align2::LEFT_TOP,
                        d::PLUGGED_IN,
                        theme::structure(size::NOTE),
                        t.ink_muted,
                    );
                }
                widgets::focus_ring(ui, &t, &resp, 2.0);
                app.stop(resp.id, true);
                if resp.clicked() {
                    if card.kind == d::MISSING.kind {
                        app.panel.mode = Some(Mode::Missing);
                        if !app.panel.request.busy {
                            app.panel.request.said = None;
                        }
                    } else if let Some(kind) = d::kinds().find(|k| *k == card.kind) {
                        choose_device(app, kind);
                    }
                }
                // Beside the card, never inside it: the asterisk and the badge.
                let corner = Rect::from_min_size(
                    rect.right_top() - Vec2::new(26.0, -2.0),
                    Vec2::new(24.0, 24.0),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(corner), |ui| {
                    if card.unproven {
                        asterisk(app, ui, card.kind);
                    }
                });
                if card.kind == d::MISSING.kind {
                    let badge = Rect::from_min_size(
                        rect.right_bottom() - Vec2::new(52.0, 22.0),
                        Vec2::new(50.0, 20.0),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(badge), |ui| {
                        widgets::beta_badge(ui, &t)
                    });
                }
            }
        });
        ui.add_space(12.8);
    }
    // An asterisk's pinned note opens under the cards.
    asterisk_notes(
        app,
        ui,
        cards
            .iter()
            .filter(|c| c.unproven)
            .map(|c| c.kind.to_string())
            .collect(),
    );
}

fn draw_calm(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let kind = app.reader_device.flatten().unwrap_or("kindle");
    let offer = offer_for(
        kind,
        app.host.platform,
        app.panel.drawn.as_ref(),
        app.panel.kindle.as_ref(),
    );
    app.panel.labelled_with = app.panel.kindle.as_ref().map(|k| k.extension.clone());
    let eject = app.send_result.as_ref().and_then(|s| s.eject.clone());
    let brass = if eject.is_none() {
        offer.as_ref().and_then(|o| o.brass.clone())
    } else {
        None
    };
    let live = enabled(app);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = layout::SPACE[3];
        if let Some(e) = &eject {
            let b = widgets::button(ui, &t, Look::Brass, &eject_label(e), live);
            app.stop(b.id, live);
            if b.clicked() {
                eject_now(app);
            }
            if eject_is_beta(app.host.platform) {
                widgets::beta_badge(ui, &t);
            }
        }
        if let Some(act) = &brass {
            let b = widgets::button(ui, &t, Look::Brass, &act.label, live);
            app.stop(b.id, live);
            if b.clicked() {
                choose_route(app, &act.route.clone());
            }
            if kindle_file_is_beta(&act.route, app.panel.kindle.as_ref()) {
                widgets::beta_badge(ui, &t);
            }
            if r::is_device_route_key(&act.route.key) {
                plugged_in_mark(ui, &t);
            }
        }
        if offer.as_ref().is_some_and(|o| o.unproven) {
            asterisk(app, ui, kind);
        }
    });
    asterisk_notes(app, ui, vec![kind.to_string()]);
    // The Kindle file building: its line and a bar that keeps moving.
    if let Some(b) = book(app)
        && let Some(build) = app.panel.builds.get(&b).cloned()
    {
        ui.add_space(layout::SPACE[3]);
        widgets::live(
            ui,
            Id::new("kindle-build"),
            build_line(build.extension.as_deref()),
            theme::prose(size::CAPTION),
            t.ink_muted,
        );
        let rect = widgets::track(ui, &t, 0.0);
        let now = app.now();
        let painter = ui.painter().with_clip_rect(rect);
        if app.reduced_motion() {
            let k = (((now / 1600.0) % 2.0) - 1.0).abs() as f32;
            painter.rect_filled(rect, 0.0, t.brass.gamma_multiply(0.35 + 0.65 * k));
        } else {
            let phase = ((now % 1400.0) / 1400.0) as f32;
            let w = rect.width() * 0.3;
            let x = rect.left() - w + phase * (rect.width() + w);
            painter.rect_filled(
                Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(w, rect.height())),
                0.0,
                t.brass,
            );
        }
        ui.ctx().request_repaint();
    }
    if let Some(fault) = app.panel.routes_fault.clone() {
        widgets::text(ui, &fault, theme::structure(size::UI), t.ink);
    } else if let Some(o) = &offer {
        for line in &o.lines {
            widgets::caption(ui, &t, line, false);
        }
    }
    ui.add_space(layout::SPACE[2]);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = layout::SPACE[4];
        let ways = widgets::button(ui, &t, Look::Outline, d::OTHER_WAYS, true);
        ui.ctx()
            .accesskit_node_builder(ways.id, |n| n.set_expanded(app.panel.ways_open));
        app.stop(ways.id, true);
        if ways.clicked() {
            app.panel.ways_open = !app.panel.ways_open;
            if !app.panel.ways_open {
                app.panel.kfx_open = false;
            }
        }
        let different = widgets::button(ui, &t, Look::Quiet, d::DIFFERENT, true);
        app.stop(different.id, true);
        if different.clicked() {
            app.panel.mode = Some(Mode::Cards);
        }
    });
    if app.panel.ways_open {
        let offer = offer.map(|o| {
            if eject.is_some() {
                d::Offer { brass: None, ..o }
            } else {
                o
            }
        });
        draw_other_ways(app, ui, offer.as_ref());
    }
}

fn draw_other_ways(app: &mut App, ui: &mut Ui, offer: Option<&d::Offer>) {
    let t = app.tokens();
    ui.add_space(layout::SPACE[3]);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().min.y,
        egui::Stroke::new(1.0, t.hole),
    );
    ui.add_space(layout::SPACE[4]);
    widgets::heading(
        ui,
        &widgets::caps(d::SEND_IT),
        theme::structure(size::NOTE),
        t.ink_muted,
    );
    for item in fold_unplugged(&send_it_rows(app.panel.drawn.as_ref(), offer)) {
        match item {
            WayItem::Row(route) => route_row(app, ui, &route),
            WayItem::Fold(fold) => {
                ui.add_space(layout::SPACE[4]);
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in fold.names.iter().enumerate() {
                        widgets::text(ui, name, theme::structure(size::NOTE), t.ink_muted);
                        if let Some(kind) = fold.kinds.get(i)
                            && d::unproven(kind, app.host.platform)
                        {
                            asterisk(app, ui, kind);
                        }
                        let sep = if i + 2 < fold.names.len() {
                            ","
                        } else if i + 2 == fold.names.len() {
                            " and"
                        } else {
                            ""
                        };
                        if !sep.is_empty() {
                            widgets::text(ui, sep, theme::structure(size::NOTE), t.ink_muted);
                        }
                    }
                    widgets::text(
                        ui,
                        fold.tail.trim_start_matches(':').trim(),
                        theme::structure(size::NOTE),
                        t.ink_muted,
                    );
                });
                asterisk_notes(app, ui, fold.kinds.clone());
            }
        }
    }
    ui.add_space(layout::SPACE[6]);
    widgets::heading(
        ui,
        &widgets::caps(d::SAVE_A_FILE),
        theme::structure(size::NOTE),
        t.ink_muted,
    );
    for row in save_rows(app.script.as_ref(), app.panel.kindle.as_ref()) {
        ui.add_space(layout::SPACE[4]);
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    widgets::text(
                        ui,
                        row.label,
                        theme::structure_bold(size::UI),
                        if row.available { t.ink } else { t.ink_muted },
                    );
                    if row.beta {
                        widgets::beta_badge(ui, &t);
                    }
                });
                widgets::text(ui, row.who, theme::structure(size::NOTE), t.ink_muted);
                if let Some(fix) = row.fix {
                    widgets::caption(ui, &t, fix, false);
                }
                if row.setup {
                    let b = widgets::button(ui, &t, Look::Quiet, d::SET_IT_UP, true);
                    app.stop(b.id, true);
                    if b.clicked() {
                        open_kfx(app);
                    }
                }
            });
            if row.available {
                let live = enabled(app);
                let save = widgets::button(ui, &t, Look::Outline, "Save…", live);
                ui.ctx().accesskit_node_builder(save.id, |n| {
                    n.set_label(format!("Save {}…", row.label))
                });
                app.stop(save.id, live);
                if save.clicked() {
                    save_file(app, row.kind);
                }
            }
        });
    }
    if app.panel.kfx_open {
        draw_kfx(app, ui);
    }
    convert::draw_reveal_foot(app, ui);
}

fn route_row(app: &mut App, ui: &mut Ui, route: &Route) {
    let t = app.tokens();
    let lines = route_lines(route);
    let kind = d::kind_of_route(route);
    ui.add_space(layout::SPACE[4]);
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                widgets::text(
                    ui,
                    &lines.title,
                    theme::structure_bold(size::UI),
                    if route.available { t.ink } else { t.ink_muted },
                );
                if kindle_file_is_beta(route, app.panel.kindle.as_ref()) {
                    widgets::beta_badge(ui, &t);
                }
                if let Some(kind) = &kind
                    && d::unproven(kind, app.host.platform)
                {
                    asterisk(app, ui, kind);
                }
            });
            widgets::text(ui, &lines.detail, theme::structure(size::NOTE), t.ink_muted);
            if let Some(where_) = &lines.where_ {
                widgets::text(ui, where_, theme::structure(size::NOTE), t.ink_muted);
            }
            if let Some(hint) = email_setup_hint(route) {
                ui.horizontal_wrapped(|ui| {
                    widgets::caption(ui, &t, hint, false);
                    let live = enabled(app);
                    let b = widgets::button(ui, &t, Look::Quiet, r::SETUP_BUTTON, live);
                    app.stop(b.id, live);
                    if b.clicked() {
                        perform(
                            app,
                            r::EMAIL_SETUP.into(),
                            r::SETUP_TITLE.into(),
                            Performer::Setup,
                        );
                    }
                });
            }
        });
        if route.available {
            let live = enabled(app);
            let b = widgets::button(
                ui,
                &t,
                Look::Outline,
                &button_label(route, app.panel.kindle.as_ref()),
                live,
            );
            app.stop(b.id, live);
            if b.clicked() {
                choose_route(app, route);
            }
        }
    });
    if let Some(kind) = kind {
        asterisk_notes(app, ui, vec![kind]);
    }
}

fn draw_missing(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    widgets::prose(ui, &t, d::MISSING_LINE);
    ui.add_space(layout::SPACE[4]);
    let label = widgets::text(
        ui,
        &widgets::caps(q::LABEL),
        theme::structure_bold(size::NOTE),
        t.ink_muted,
    );
    ui.horizontal_wrapped(|ui| {
        let busy = app.panel.request.busy;
        let field = ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut app.panel.request.draft)
                .char_limit(q::MAX_DEVICE)
                .font(theme::structure(size::LABEL))
                .desired_width(220.0),
        );
        let field = field.labelled_by(label.id);
        app.stop(field.id, !busy);
        let words = if app.panel.request.said.as_ref().is_some_and(|s| s.1) {
            q::TRY_AGAIN
        } else {
            q::SEND
        };
        let can = !busy && !app.panel.request.draft.trim().is_empty();
        let send = widgets::button(ui, &t, Look::Outline, words, can);
        app.stop(send.id, can);
        if send.clicked() {
            send_device_request(app);
        }
        widgets::beta_badge(ui, &t);
    });
    widgets::caption(ui, &t, q::WHAT_IS_SENT, false);
    let (said, bad) = app.panel.request.said.clone().unwrap_or_default();
    widgets::live(
        ui,
        Id::new("request-status"),
        &said,
        theme::prose(size::CAPTION),
        if bad { t.alarm } else { t.ink_muted },
    );
    ui.add_space(layout::SPACE[2]);
    ui.horizontal(|ui| {
        let save = widgets::button(ui, &t, Look::Quiet, "Save the EPUB…", true);
        let back = widgets::button(ui, &t, Look::Quiet, "Back", true);
        app.stop(save.id, true);
        app.stop(back.id, true);
        if save.clicked() {
            save_file(app, "epub");
        }
        if back.clicked() {
            app.panel.mode = Some(Mode::Cards);
        }
    });
}

/// The mark on what nobody has run on that hardware: a press pins its
/// note open (expanded state exposed); hovering shows it.
fn asterisk(app: &mut App, ui: &mut Ui, kind: &str) {
    let t = app.tokens();
    let Some(_) = d::asterisk_note(kind, app.host.platform) else {
        return;
    };
    let pinned = app.panel.pinned.contains(kind);
    let galley =
        ui.painter()
            .layout_no_wrap("*".into(), theme::structure_bold(size::LEDE), t.ink_muted);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(6.0, 0.0), Sense::click());
    let resp = ui
        .interact(rect, Id::new(("asterisk", kind)), Sense::click())
        .union(resp);
    let name = format!("About {}: never tried on real hardware", d::name_of(kind));
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &name));
    ui.ctx()
        .accesskit_node_builder(resp.id, |n| n.set_expanded(pinned));
    let ink = if resp.hovered() || pinned {
        t.ink
    } else {
        t.ink_muted
    };
    ui.painter()
        .galley(rect.min + Vec2::new(3.0, 0.0), galley, ink);
    widgets::focus_ring(ui, &t, &resp, 2.0);
    app.stop(resp.id, true);
    if resp.clicked() {
        if pinned {
            app.panel.pinned.remove(kind);
        } else {
            app.panel.pinned.insert(kind.to_string());
        }
    }
    if resp.hovered() {
        ui.memory_mut(|m| m.data.insert_temp(Id::new(("asterisk-hover", kind)), true));
    } else {
        ui.memory_mut(|m| m.data.remove::<bool>(Id::new(("asterisk-hover", kind))));
    }
}

/// The open notes of these kinds' asterisks, with It worked / It did not
/// work where there is a route to report on.
fn asterisk_notes(app: &mut App, ui: &mut Ui, kinds: Vec<String>) {
    let t = app.tokens();
    for kind in kinds {
        let hovered = ui.memory(|m| {
            m.data
                .get_temp::<bool>(Id::new(("asterisk-hover", &kind)))
                .unwrap_or(false)
        });
        if !app.panel.pinned.contains(&kind) && !hovered {
            continue;
        }
        let Some(note) = d::asterisk_note(&kind, app.host.platform) else {
            continue;
        };
        egui::Frame::NONE
            .fill(t.paper)
            .stroke(egui::Stroke::new(1.0, t.hole))
            .corner_radius(theme::RADIUS_WELL)
            .inner_margin(egui::Margin::symmetric(13, 10))
            .show(ui, |ui| {
                widgets::text(ui, &note, theme::prose(size::CAPTION), t.ink_soft);
                if report_route(&kind, app.host.platform).is_some() {
                    let rep = app.panel.reports.get(&kind).cloned().unwrap_or_default();
                    ui.horizontal_wrapped(|ui| {
                        match &rep.said {
                            Some((_, false)) => {}
                            Some((_, true)) => {
                                let b =
                                    widgets::button(ui, &t, Look::Quiet, q::TRY_AGAIN, !rep.busy);
                                if b.clicked()
                                    && let Some(last) = rep.last
                                {
                                    send_report(app, &kind, last);
                                }
                            }
                            None => {
                                let yes =
                                    widgets::button(ui, &t, Look::Quiet, q::WORKED, !rep.busy);
                                let no = widgets::button(
                                    ui,
                                    &t,
                                    Look::Quiet,
                                    q::DID_NOT_WORK,
                                    !rep.busy,
                                );
                                if yes.clicked() {
                                    send_report(app, &kind, true);
                                }
                                if no.clicked() {
                                    send_report(app, &kind, false);
                                }
                            }
                        }
                        widgets::beta_badge(ui, &t);
                        let (said, bad) = rep.said.clone().unwrap_or_default();
                        widgets::live(
                            ui,
                            Id::new(("report-status", &kind)),
                            &said,
                            theme::prose(size::CAPTION),
                            if bad { t.alarm } else { t.ink_muted },
                        );
                    });
                }
            });
    }
}

fn draw_kfx(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let busy = app.panel.kfx.installing || app.panel.sending || !app.panel.builds.is_empty();
    let shown = show_setup(
        app.panel.kfx.setup.as_ref(),
        Some(&[]),
        app.panel.kfx.just_installed,
        app.panel.kfx.installing,
    );
    if !shown {
        return;
    }
    let Some(setup) = app.panel.kfx.setup.clone() else {
        return;
    };
    ui.add_space(layout::SPACE[5]);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().min.y,
        egui::Stroke::new(1.0, t.hole),
    );
    ui.add_space(layout::SPACE[4]);
    let block = ui.scope(|ui| {
        widgets::state_label(ui, &t, k::HEADING);
        widgets::prose(ui, &t, &setup.summary);
        for step in &setup.steps {
            ui.add_space(layout::SPACE[4]);
            ui.horizontal_wrapped(|ui| {
                widgets::text(ui, &step.name, theme::structure_bold(size::UI), t.ink);
                match control_for(step, busy) {
                    Control::Status(words) => {
                        widgets::text(
                            ui,
                            &widgets::caps(&words),
                            theme::structure(size::FINE),
                            t.ink_muted,
                        );
                    }
                    Control::Link { label, url } => {
                        let b = widgets::button(ui, &t, Look::Outline, &label, true);
                        app.stop(b.id, true);
                        if b.clicked() && !app.host.open(&url) {
                            app.panel.kfx.status = Status::failed(&k::link_failed_line(&url));
                        }
                    }
                    Control::Install { label, disabled } => {
                        let words = if app.panel.kfx.installing {
                            "Installing…".to_string()
                        } else {
                            label
                        };
                        let b = widgets::button(ui, &t, Look::Brass, &words, !disabled);
                        app.stop(b.id, !disabled);
                        if b.clicked() {
                            kfx_install(app);
                        }
                    }
                }
            });
        }
        let status = app.panel.kfx.status.clone();
        widgets::live(
            ui,
            Id::new("kfx-status"),
            &status.line,
            theme::prose(size::CAPTION),
            if status.bad { t.alarm } else { t.ink_muted },
        );
    });
    ui.ctx().accesskit_node_builder(block.response.id, |n| {
        n.set_role(accesskit::Role::Region);
        n.set_label(k::HEADING);
    });
}
