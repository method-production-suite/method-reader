//! The window, photographed for the README and the site: what
//! `cargo xtask capture-screens` runs, once per picture, as
//! examples/capture.rs (egui_kittest draws the real App offscreen through
//! wgpu; it is a dev-dependency, so the drawing half lives in the example
//! and everything it decides lives here, where the tests reach it).
//!
//! One run is one picture: one shot (`drop`, `result` or `read`) in one
//! theme at one size. The window runs on the real engine linked in, which
//! reads the process environment, so the xtask starts this program with
//! the run's own: a scratch library, scratch app settings, no volumes, a
//! reMarkable address nothing listens on, a scratch COPY of Calibre's
//! settings. [`parse_args`] refuses to start without
//! METHOD_READER_CONFIG_DIR and METHOD_READER_LIBRARY (or, each unset, the
//! LEGACY SCREEPUB_ name the engine falls back to), and [`Guard`] holds
//! the line the old capture
//! gate held, in code: the demo PDF is the only file it converts, the
//! scratch library the only place it reads a book from, and nothing that
//! copies, sends, builds, ejects, exports or installs gets through. The
//! two calls that would start Calibre to ask a question (the Kindle file
//! check and the KFX checklist) are answered "kept out", as a machine
//! without Calibre would answer them; any other call outside the line is
//! refused AND fails the picture.
//!
//! [`capture_host`] is a computer with no dialogs, no browser, no network,
//! no updater, no sound and no storage file, so nothing on the screen
//! depends on this machine's own state, and [`reach`] drives the window
//! into the shot the way a person would (a file dropped on the well, the
//! Read tab), waits for it to settle, and says what went wrong if it never
//! does.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::bookview::Place;
use crate::app::host::{FakeDialogs, FakeOpener, FakePoster, FakeSpeaker, Host, NoUpdater};
use crate::app::{App, convert, panel};
use crate::engine::{Engine, Reply, Request};
use crate::model::convert::Platform;
use crate::model::frame::Surface;
use crate::model::intent::Intent;
use crate::model::update::MemoryStorage;

/// The window's pictures: what each one shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shot {
    /// The idle drop well.
    Drop,
    /// The demo script converted: the send panel's device cards beside the
    /// book's first page.
    Result,
    /// The same book on Read, with its scene index and the script.
    Read,
}

impl Shot {
    pub fn word(self) -> &'static str {
        match self {
            Shot::Drop => "drop",
            Shot::Result => "result",
            Shot::Read => "read",
        }
    }

    fn from_word(w: &str) -> Option<Shot> {
        [Shot::Drop, Shot::Result, Shot::Read]
            .into_iter()
            .find(|s| s.word() == w)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn word(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn egui(self) -> egui::Theme {
        match self {
            Theme::Light => egui::Theme::Light,
            Theme::Dark => egui::Theme::Dark,
        }
    }
}

/// Every picture is drawn at two pixels a point, as Chrome's
/// deviceScaleFactor 2 drew the old window: a 1280 by 800 window is a
/// 2560 by 1600 picture, which is what frame.html expects.
pub const PIXELS_PER_POINT: f32 = 2.0;

/// The longest a shot may take to reach its state.
pub const REACH_LIMIT: Duration = Duration::from_secs(120);

/// One picture's orders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    pub shot: Shot,
    pub theme: Theme,
    /// Points.
    pub width: u32,
    pub height: u32,
    /// The one file this run may convert.
    pub demo: PathBuf,
    /// Where the picture's raw RGBA goes: width x height x 4 bytes, row by
    /// row, at [`PIXELS_PER_POINT`].
    pub out: PathBuf,
    /// METHOD_READER_LIBRARY (or SCREEPUB_LIBRARY), the only place a book
    /// may be read from.
    pub library: PathBuf,
    /// `--panel read|edit` (optional, read): which half of the send
    /// panel's Read it | Edit it the result shows.
    pub panel: Intent,
}

pub const USAGE: &str = "usage: capture --shot drop|result|read --theme light|dark \
     --width <points> --height <points> --demo <file.pdf> --out <file.rgba> \
     [--panel read|edit]\n\
     with METHOD_READER_CONFIG_DIR and METHOD_READER_LIBRARY set to scratch folders";

/// The orders, read from the arguments and the environment. Every flag is
/// required, once; anything else is refused.
pub fn parse_args(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Args, String> {
    use crate::model::names::{
        CONFIG_DIR_VAR, LEGACY_CONFIG_DIR_VAR, LEGACY_LIBRARY_VAR, LIBRARY_VAR,
    };
    // The folder the engine will use: the new name when it is set at all
    // (a blank one is refused, never passed over), else the old one.
    let scratch = |var: &str, legacy: &str| -> Result<String, String> {
        match env(var).or_else(|| env(legacy)) {
            Some(v) if !v.trim().is_empty() => Ok(v),
            _ => Err(format!(
                "capture: set {var} to a scratch folder first; this never runs against \
                 the real settings or the real library"
            )),
        }
    };
    scratch(CONFIG_DIR_VAR, LEGACY_CONFIG_DIR_VAR)?;
    let library = PathBuf::from(scratch(LIBRARY_VAR, LEGACY_LIBRARY_VAR)?);
    const NAMES: [&str; 7] = ["shot", "theme", "width", "height", "demo", "out", "panel"];
    let mut got: [Option<String>; 7] = Default::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let Some(name) = a.strip_prefix("--") else {
            return Err(format!("capture: unexpected argument '{a}'"));
        };
        let Some(i) = NAMES.iter().position(|n| *n == name) else {
            return Err(format!("capture: unknown option '--{name}'"));
        };
        if got[i].is_some() {
            return Err(format!("capture: --{name} given twice"));
        }
        let value = it
            .next()
            .ok_or_else(|| format!("capture: --{name} needs a value"))?;
        got[i] = Some(value.clone());
    }
    let [shot, theme, width, height, demo, out, panel] = got;
    let panel = match panel.as_deref() {
        None | Some("read") => Intent::Read,
        Some("edit") => Intent::Edit,
        Some(other) => return Err(format!("capture: no panel named {other}")),
    };
    let need =
        |v: Option<String>, name: &str| v.ok_or_else(|| format!("capture: --{name} is required"));
    let shot = need(shot, "shot")?;
    let shot = Shot::from_word(&shot).ok_or_else(|| format!("capture: no shot named {shot}"))?;
    let theme = match need(theme, "theme")?.as_str() {
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        other => return Err(format!("capture: no theme named {other}")),
    };
    let points = |v: Option<String>, name: &str| -> Result<u32, String> {
        let v = need(v, name)?;
        v.parse::<u32>()
            .ok()
            .filter(|n| (100..=4000).contains(n))
            .ok_or_else(|| format!("capture: --{name} {v} is not a window size in points"))
    };
    let width = points(width, "width")?;
    let height = points(height, "height")?;
    let demo = PathBuf::from(need(demo, "demo")?);
    if !demo.is_absolute() || !demo.is_file() {
        return Err(format!(
            "capture: --demo {} is not a file named by its whole path",
            demo.display()
        ));
    }
    let out = PathBuf::from(need(out, "out")?);
    Ok(Args {
        shot,
        theme,
        width,
        height,
        demo,
        out,
        library,
        panel,
    })
}

/// What [`Guard`] does with one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Run it on the real engine.
    Run,
    /// Answer it as a machine without Calibre would: the question would
    /// start Calibre, which runs only inside tests that isolate it.
    KeepOut,
    /// Never run it, and fail the picture: not a call the window makes on
    /// the way to one.
    Refuse,
}

/// `path` with `.` and `..` folded away, never above the root, as the
/// parts the file system compares: on Windows a drive or share is the
/// same written plainly or verbatim (`C:` and `\\?\C:`, which is what
/// canonicalize answers), and names match whatever their case.
fn folded(path: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never above the root (or the drive and root).
                if out.len() > root_parts(path) {
                    out.pop();
                }
            }
            Component::Prefix(p) => out.push(prefix_key(p.kind())),
            Component::RootDir => out.push(std::path::MAIN_SEPARATOR_STR.to_owned()),
            Component::Normal(n) => out.push(name_key(&n.to_string_lossy())),
        }
    }
    out
}

/// How many leading parts of `path` are its prefix and root.
fn root_parts(path: &Path) -> usize {
    path.components()
        .take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
        .count()
}

#[cfg(windows)]
fn prefix_key(p: std::path::Prefix<'_>) -> String {
    use std::path::Prefix;
    match p {
        Prefix::Disk(d) | Prefix::VerbatimDisk(d) => {
            format!("{}:", char::from(d).to_ascii_lowercase())
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => format!(
            "\\\\{}\\{}",
            name_key(&server.to_string_lossy()),
            name_key(&share.to_string_lossy())
        ),
        Prefix::Verbatim(x) => format!("\\\\?\\{}", name_key(&x.to_string_lossy())),
        Prefix::DeviceNS(x) => format!("\\\\.\\{}", name_key(&x.to_string_lossy())),
    }
}

#[cfg(not(windows))]
fn prefix_key(_: std::path::Prefix<'_>) -> String {
    unreachable!("only Windows paths have a prefix")
}

#[cfg(windows)]
fn name_key(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(not(windows))]
fn name_key(name: &str) -> String {
    name.to_owned()
}

/// Whether `path` is a whole path strictly inside `dir`, as written or as
/// the file system resolves both (a temp folder is often a symlink).
pub fn inside(path: &str, dir: &Path) -> bool {
    let p = Path::new(path);
    if path.is_empty() || !p.is_absolute() {
        return false;
    }
    let lexical = |p: &Path, d: &Path| {
        let (p, d) = (folded(p), folded(d));
        p != d && p.starts_with(&d)
    };
    if lexical(p, dir) {
        return true;
    }
    match (std::fs::canonicalize(p), std::fs::canonicalize(dir)) {
        (Ok(p), Ok(d)) => lexical(&p, &d),
        _ => false,
    }
}

/// Whether `path` names the demo PDF itself.
pub fn is_demo(path: &str, demo: &Path) -> bool {
    let p = Path::new(path);
    if !p.is_absolute() {
        return false;
    }
    if folded(p) == folded(demo) {
        return true;
    }
    match (std::fs::canonicalize(p), std::fs::canonicalize(demo)) {
        (Ok(a), Ok(b)) => folded(&a) == folded(&b),
        _ => false,
    }
}

/// The line, call by call. Every allowed call lists or reads, or converts
/// the demo script into the scratch library.
pub fn verdict(request: &Request, demo: &Path, library: &Path) -> Verdict {
    match request {
        Request::Version | Request::AppSettings | Request::Devices { .. } => Verdict::Run,
        Request::Convert { path, options, .. } if is_demo(path, demo) && options.is_none() => {
            Verdict::Run
        }
        Request::Settings {
            fountain,
            set: None,
        } if inside(fountain, library) => Verdict::Run,
        Request::KindleCheck { .. } | Request::KfxStatus => Verdict::KeepOut,
        _ => Verdict::Refuse,
    }
}

/// The real engine behind the line.
pub struct Guard {
    inner: Arc<dyn Engine>,
    demo: PathBuf,
    library: PathBuf,
    refused: Arc<Mutex<Vec<String>>>,
}

impl Guard {
    pub fn new(inner: Arc<dyn Engine>, demo: PathBuf, library: PathBuf) -> Self {
        Self {
            inner,
            demo,
            library,
            refused: Arc::default(),
        }
    }

    /// Every call refused so far, as its command line.
    pub fn refused(&self) -> Arc<Mutex<Vec<String>>> {
        self.refused.clone()
    }
}

fn no(message: &str) -> Reply {
    Reply::ok(json!({"ok": false, "error": {"code": "internal", "message": message}}))
}

impl Engine for Guard {
    fn run(&self, request: &Request, progress: &mut dyn FnMut(Value)) -> Reply {
        match verdict(request, &self.demo, &self.library) {
            Verdict::Run => self.inner.run(request, progress),
            Verdict::KeepOut => no("Calibre is kept out of the capture."),
            Verdict::Refuse => {
                let line = request.argv().join(" ");
                if let Ok(mut r) = self.refused.lock() {
                    r.push(line.clone());
                }
                no(&format!(
                    "capture: {line}: not a call the window makes on the way to a picture"
                ))
            }
        }
    }
}

/// A computer that answers nothing of its own: no dialog, no browser, no
/// network, no updater (so no quiet checks and no update label, as the
/// old capture's window, which had no updater, drew it), no sound, a
/// storage that starts empty and is never written to disk, and the real
/// clock, so the window's motion runs and settles as it would.
pub fn capture_host() -> Host {
    let start = Instant::now();
    Host {
        dialogs: Box::new(FakeDialogs::default()),
        opener: Box::new(FakeOpener::default()),
        poster: Arc::new(FakePoster::default()),
        storage: Box::new(MemoryStorage::default()),
        clock: Box::new(move || start.elapsed().as_secs_f64() * 1000.0),
        platform: Platform::current(),
        reduced_motion: Arc::new(AtomicBool::new(false)),
        updater: Arc::new(NoUpdater),
        speaker: Box::new(FakeSpeaker::default()),
    }
}

/// What [`reach`] drives: the window, one frame at a time.
pub trait Stepper {
    /// One frame.
    fn step(&mut self);
    fn app(&self) -> &App;
    fn app_mut(&mut self) -> &mut App;
}

/// How long a settled window is stepped before its picture: the tabs
/// arrive over 0.2 s, staggered by up to 0.14 s, and anything else that
/// eases in is quicker.
pub const SETTLE: Duration = Duration::from_millis(1500);

const FRAME: Duration = Duration::from_millis(16);

fn until(
    s: &mut dyn Stepper,
    what: &str,
    limit: Instant,
    done: &dyn Fn(&App) -> Result<bool, String>,
) -> Result<(), String> {
    loop {
        if done(s.app())? {
            return Ok(());
        }
        if Instant::now() >= limit {
            return Err(format!("capture: timed out waiting for {what}"));
        }
        std::thread::sleep(FRAME);
        s.step();
    }
}

/// The book's conversion, done; a refusal says what the window said.
fn converted(app: &App) -> Result<bool, String> {
    if app.convert.view == convert::View::Failed {
        let r = app.convert.refusal.as_ref();
        return Err(format!(
            "capture: the window refused the demo script: {}: {} ({})",
            r.map_or("", |r| r.heading),
            r.map_or("", |r| r.message.as_str()),
            r.map_or("no code", |r| r.code.as_str()),
        ));
    }
    Ok(app.convert.view == convert::View::Done && app.show.is_none() && app.quiet())
}

/// Drive the window into `shot` and let it settle. `demo` is dropped on
/// the well as a file from the desktop would be.
pub fn reach(s: &mut dyn Stepper, shot: Shot, demo: &str, limit: Duration) -> Result<(), String> {
    reach_with(s, shot, Intent::Read, demo, limit)
}

/// [`reach`], with the send panel on `panel`'s half: chosen as a click
/// would choose it, less the write to app settings (the picture is of the
/// panel, not the file).
pub fn reach_with(
    s: &mut dyn Stepper,
    shot: Shot,
    panel: Intent,
    demo: &str,
    limit: Duration,
) -> Result<(), String> {
    let limit = Instant::now() + limit;
    s.app_mut().picture_mode = true;
    s.step();
    until(s, "the drop well", limit, &|app| {
        Ok(app.convert.view == convert::View::Idle && app.convert.library.is_some() && app.quiet())
    })?;
    if shot != Shot::Drop {
        convert::drop_paths(s.app_mut(), &[demo.to_string()]);
        until(s, "the finished book", limit, &converted)?;
    }
    match shot {
        Shot::Drop => {}
        Shot::Result => {
            // The first-time question, never the calm screen: the scratch
            // settings hold no reader's device, so the panel shows the
            // device cards. The calm screen would be drawn from what this
            // machine has (its apps, Calibre) and differ from one computer
            // to the next.
            until(s, "the device cards", limit, &|app| match app.panel.mode {
                Some(panel::Mode::Calm) => Err(
                    "capture: the result shows the calm screen, so the capture's settings name \
                     a reader; the picture would depend on this machine's apps"
                        .into(),
                ),
                Some(panel::Mode::Cards) => Ok(true),
                _ => Ok(false),
            })?;
            until(s, "the first page", limit, &|app| {
                Ok(app.views.peek(Place::FirstPage).is_some())
            })?;
            s.app_mut().panel.intent = panel;
        }
        Shot::Read => {
            s.app_mut().go_to(Surface::Read);
            until(s, "the scene index and the script", limit, &|app| {
                Ok(app.surface == Surface::Read
                    && app.views.peek(Place::Read).is_some()
                    && !app.read.marks.is_empty())
            })?;
        }
    }
    let settle = Instant::now() + SETTLE;
    while Instant::now() < settle {
        std::thread::sleep(FRAME);
        s.step();
    }
    until(s, "the engine to go quiet", limit, &|app| Ok(app.quiet()))?;
    s.step();
    Ok(())
}
