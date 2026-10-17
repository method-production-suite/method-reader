//! One capture run, from claiming the demo library to putting it back (was
//! tools/capture/run.ts).
//!
//! Everything a run creates, it removes: Chrome, the page server, the demo
//! library, the first-pass pictures, the copy of Calibre's settings, and
//! the folders above the library that it had to make (and only those, and
//! only if they are empty). The window program that draws pass one is
//! stopped by its backend before the backend returns, whatever happened. Cleanup runs whether
//! the run succeeds, fails or is interrupted, each step on its own so one
//! failure cannot skip the rest, and a cleanup failure is reported after
//! the error that ended the run, never instead of it.
//!
//! Nothing is written to the repository until every picture has been taken
//! (the window refused no engine call of its own) and the page server hit
//! no error,
//! so a run that dies half-way leaves the committed pictures exactly as
//! they were. The library's marker names the run's pid, so two runs at once
//! cannot delete each other's library.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{Value, json};

use super::backend::{
    Browser, CaptureOptions, CommandLine, Exchange, PassOne, Terminal, WindowBackend,
    terminal_query,
};
use super::cdp::make_temp_dir;
use super::http::{Response, Server};
use super::server::{Handler, HandlerOptions, Records, snapshot};
use super::shots::{Kind, Shot, framed_size, outputs_for, write_if_changed};
use crate::calibre_config::{copy_calibre_config, this_host_real_config};

/// The file that says a library is the capture tool's. Without it, a
/// library found at the demo path belongs to somebody and is never touched.
pub const MARKER: &str = ".method-reader-capture";

/// Why a run ended without its pictures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunError {
    pub message: String,
    /// A failure the tool expected and can name: its message says it all.
    /// Otherwise something unforeseen, reported as it came.
    pub expected: bool,
    /// The signal that stopped the run (2 or 15), if one did.
    pub interrupted: Option<usize>,
    /// What went wrong while cleaning up after it, to report after it.
    pub cleanup: Vec<String>,
}

impl RunError {
    pub fn expected(message: impl Into<String>) -> RunError {
        RunError {
            message: message.into(),
            expected: true,
            interrupted: None,
            cleanup: Vec::new(),
        }
    }

    pub fn other(message: impl Into<String>) -> RunError {
        RunError {
            expected: false,
            ..RunError::expected(message)
        }
    }
}

pub type Launch = Box<dyn FnOnce() -> Result<Box<dyn Browser>, String>>;

pub struct RunOptions {
    pub shots: Vec<Shot>,
    pub repo_dir: PathBuf,
    pub demo_pdf: PathBuf,
    /// The demo library. The result screen prints paths inside it.
    pub library: PathBuf,
    pub launch: Launch,
    /// What draws pass one of a window shot: [`super::backend::EguiWindow`].
    pub window: Box<dyn WindowBackend>,
    /// What runs the command line for the terminal shot:
    /// [`super::backend::RealCli`].
    pub cli: Box<dyn CommandLine>,
    /// Where the pictures go: the repository, outside tests.
    pub out_dir: PathBuf,
    /// Where the first-pass pictures and the Calibre copy are kept.
    /// Defaults to the OS temp folder.
    pub tmp: Option<PathBuf>,
    /// The Calibre settings folder to COPY for the engine (never handed to
    /// it itself). Defaults to the one Calibre would use.
    pub calibre_config: Option<PathBuf>,
    pub log: Box<dyn FnMut(&str)>,
    /// Set to a signal's number (2, 15) by a handler: the run stops at its
    /// next look, cleans up, and answers the signal.
    pub interrupt: Option<Arc<AtomicUsize>>,
}

/// The Calibre settings folder this machine's Calibre would use, by
/// Calibre's own rule (crate::calibre_config). The run copies it.
pub fn default_calibre_config() -> PathBuf {
    this_host_real_config()
}

/// What a marker says: which capture process made the library, and the
/// highest folder it made. `made` is trusted only if it names the library
/// itself or a folder above it, never anywhere else.
fn read_marker(marker: &Path, library: &Path) -> (Option<u32>, Option<PathBuf>) {
    let Ok(v) = std::fs::read_to_string(marker)
        .map_err(|_| ())
        .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|_| ()))
    else {
        return (None, None);
    };
    let pid = v
        .get("pid")
        .and_then(Value::as_u64)
        .filter(|p| *p > 0)
        .and_then(|p| u32::try_from(p).ok());
    let made = v
        .get("made")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|m| library.starts_with(m));
    (pid, made)
}

/// Whether a process is running. EPERM means it is, as somebody else.
pub fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        let Ok(raw) = i32::try_from(pid) else {
            return false;
        };
        !matches!(kill(Pid::from_raw(raw), None), Err(Errno::ESRCH))
    }
    #[cfg(not(unix))]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
            .unwrap_or(true)
    }
}

/// The highest folder `create_dir_all(dir)` would make: the child of the
/// deepest folder on the way that already exists.
fn highest_missing(dir: &Path) -> Option<PathBuf> {
    let mut top = None;
    for a in dir.ancestors() {
        if a.exists() {
            break;
        }
        top = Some(a.to_path_buf());
    }
    top
}

/// Make the demo library and mark it as `pid`'s. Returns the highest folder
/// the tool made on the way (the library itself, or a Documents folder
/// above it), which is as far up as cleanup may ever go.
///
/// A library that is already there is removed only if its marker names a
/// capture process that no longer exists: an interrupted run left it, and
/// whatever that run made above it is then this run's to remove too. A
/// marker naming a live process is a capture still running, and two runs
/// must not delete each other's library. Anything else is refused.
pub fn claim_library(library: &Path, pid: u32) -> Result<PathBuf, RunError> {
    let mut inherited: Option<PathBuf> = None;
    let lib = library.display();
    if library.exists() {
        let marker = library.join(MARKER);
        if !marker.exists() {
            return Err(RunError::expected(format!(
                "capture: {lib} already exists, and the capture tool did not make it \
                 (it has no {MARKER}). It is not the tool's to touch; move it and rerun."
            )));
        }
        let (found, made) = read_marker(&marker, library);
        let Some(found) = found else {
            return Err(RunError::expected(format!(
                "capture: {lib} has a {MARKER} that names no capture pid, so there is no telling \
                 whether a capture is still using it. If none is running, remove the folder and rerun."
            )));
        };
        if alive(found) {
            return Err(RunError::expected(format!(
                "capture pid {found} is using {lib}. Let it finish, or stop it, and rerun."
            )));
        }
        inherited = made;
        std::fs::remove_dir_all(library)
            .map_err(|e| RunError::other(format!("capture: could not clear {lib}: {e}")))?;
    }
    let made = highest_missing(library).unwrap_or_else(|| library.to_path_buf());
    std::fs::create_dir_all(library)
        .map_err(|e| RunError::other(format!("capture: could not make {lib}: {e}")))?;
    // Both are the library or a folder above it, so the shorter is higher.
    let top = match inherited {
        Some(i) if i.components().count() < made.components().count() => i,
        _ => made,
    };
    let marker = json!({ "pid": pid, "made": top.to_string_lossy() }).to_string();
    std::fs::write(library.join(MARKER), format!("{marker}\n"))
        .map_err(|e| RunError::other(format!("capture: could not mark {lib}: {e}")))?;
    Ok(top)
}

/// Remove each folder above the library that the tool made, bottom up,
/// stopping at the first that is not empty: something in it is not ours.
fn remove_made_parents(library: &Path, top: &Path) -> std::io::Result<()> {
    if top == library {
        return Ok(());
    }
    let mut dir = library.parent();
    while let Some(d) = dir.filter(|d| d.starts_with(top)) {
        match std::fs::remove_dir(d) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::AlreadyExists
                ) =>
            {
                return Ok(());
            }
            Err(e) => return Err(e),
        }
        dir = d.parent();
    }
    Ok(())
}

/// Everything a conversion left in the library, gone; the marker stays.
fn empty_library(library: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(library)? {
        let entry = entry?;
        if entry.file_name() == MARKER {
            continue;
        }
        let p = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&p)?;
        } else {
            std::fs::remove_file(&p)?;
        }
    }
    Ok(())
}

/// The environment the window's engine runs with, on top of this
/// process's.
///
/// - METHOD_READER_LIBRARY: the scratch library.
/// - METHOD_READER_CONFIG_DIR: a folder under the scratch library, cleared with
///   it before every shot, which reads back as no settings at all, so the
///   pictures never drift with whatever the developer last chose in their
///   own app settings. The window's guard lets nothing write into it.
/// - METHOD_READER_VOLUME_ROOTS, METHOD_READER_REMARKABLE_ENDPOINT: no real reader,
///   ever: an empty mount parent, and a reMarkable address nothing listens on.
/// - CALIBRE_CONFIG_DIRECTORY: Calibre run against its real settings
///   folder rewrites files there (gui.json, 0.7.3 QA). The window's guard
///   keeps every question that would start Calibre out, and this is the
///   second line behind it: a scratch COPY made for this run, so the real
///   folder is never handed to anything.
pub fn engine_env(library: &Path, calibre: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("METHOD_READER_LIBRARY", library.to_path_buf()),
        (
            "METHOD_READER_CONFIG_DIR",
            library.join(".capture-app-settings"),
        ),
        ("METHOD_READER_VOLUME_ROOTS", library.join(".no-volumes")),
        (
            "METHOD_READER_REMARKABLE_ENDPOINT",
            PathBuf::from("http://127.0.0.1:9"),
        ),
        ("CALIBRE_CONFIG_DIRECTORY", calibre.to_path_buf()),
    ]
}

/// The run's cleanup: every step on its own, in this order, because nothing
/// may write into the library after it is removed.
struct Cleanup<'a> {
    o: &'a RunOptions,
    top: PathBuf,
    browser: Option<Box<dyn Browser>>,
    server: Option<Server>,
    pass_one: Option<PathBuf>,
    calibre_scratch: Option<PathBuf>,
}

impl Cleanup<'_> {
    fn run(&mut self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut step = |what: &str, r: Result<(), String>| {
            if let Err(e) = r {
                errors.push(format!("capture: cleanup could not {what}: {e}"));
            }
        };
        step(
            "close Chrome",
            self.browser.as_mut().map_or(Ok(()), |b| b.close()),
        );
        step(
            "stop the page server",
            if self
                .server
                .as_mut()
                .is_none_or(|s| s.stop(Duration::from_secs(5)))
            {
                Ok(())
            } else {
                Err("it did not stop within 5 s".into())
            },
        );
        let lib = &self.o.library;
        step(&format!("remove {}", lib.display()), remove_all(lib));
        step(
            "remove the first-pass pictures",
            self.pass_one.as_deref().map_or(Ok(()), remove_all),
        );
        step(
            "remove the copy of Calibre's settings",
            self.calibre_scratch.as_deref().map_or(Ok(()), remove_all),
        );
        step(
            &format!("remove the folders it made above {}", lib.display()),
            remove_made_parents(lib, &self.top).map_err(|e| e.to_string()),
        );
        errors
    }
}

fn remove_all(p: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(p) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

fn signal_name(sig: usize) -> &'static str {
    if sig == 2 { "SIGINT" } else { "SIGTERM" }
}

/// The window that stands in once the run has taken the real one.
struct Taken;

impl WindowBackend for Taken {
    fn pass_one(&mut self, _p: PassOne<'_>) -> Result<Vec<u8>, String> {
        Err("capture: the window was already taken".into())
    }
}

impl CommandLine for Taken {
    fn transcript(&mut self, _t: Terminal<'_>) -> Result<Vec<Exchange>, String> {
        Err("capture: the command line was already taken".into())
    }
}

/// Takes every picture, then writes them all, or none.
pub fn run_capture(mut o: RunOptions) -> Result<(), RunError> {
    let launch = std::mem::replace(&mut o.launch, Box::new(|| Err("already launched".into())));
    let mut window = std::mem::replace(&mut o.window, Box::new(Taken));
    let mut cli = std::mem::replace(&mut o.cli, Box::new(Taken));
    let mut log = std::mem::replace(&mut o.log, Box::new(|_| {}));
    let top = claim_library(&o.library, std::process::id())?;
    let mut c = Cleanup {
        o: &o,
        top,
        browser: None,
        server: None,
        pass_one: None,
        calibre_scratch: None,
    };
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let result = take(
        &mut c,
        window.as_mut(),
        cli.as_mut(),
        log.as_mut(),
        &mut pictures,
        launch,
    );
    let errors = c.run();
    let interrupted = o
        .interrupt
        .as_ref()
        .map(|i| i.load(Ordering::SeqCst))
        .filter(|s| *s != 0);
    match result {
        Err(mut e) => {
            if let Some(sig) = interrupted {
                e.interrupted = Some(sig);
            }
            e.cleanup = errors;
            Err(e)
        }
        Ok(()) => {
            if let Some(sig) = interrupted {
                let mut e = RunError::expected(format!("capture: {}", signal_name(sig)));
                e.interrupted = Some(sig);
                e.cleanup = errors;
                return Err(e);
            }
            // Only now: every picture taken, nothing failed.
            // (Written even if cleanup then failed; that is said after.)
            for (out, png) in &pictures {
                let written = write_if_changed(&o.out_dir.join(out), png)
                    .map_err(|e| RunError::other(format!("capture: could not write {out}: {e}")))?;
                log(&format!("{}  {out}", written.word()));
            }
            if errors.is_empty() {
                Ok(())
            } else {
                Err(RunError::expected(errors.join("\n")))
            }
        }
    }
}

/// Everything between claiming the library and cleaning up.
fn take(
    c: &mut Cleanup<'_>,
    window: &mut dyn WindowBackend,
    cli: &mut dyn CommandLine,
    log: &mut dyn FnMut(&str),
    pictures: &mut Vec<(String, Vec<u8>)>,
    launch: Launch,
) -> Result<(), RunError> {
    let o = c.o;
    let tmp = o.tmp.clone().unwrap_or_else(std::env::temp_dir);
    let io = |what: &str| {
        let what = what.to_string();
        move |e: std::io::Error| RunError::other(format!("capture: could not {what}: {e}"))
    };
    let pass_one = c
        .pass_one
        .insert(make_temp_dir(&tmp, "method-reader-capture-").map_err(io("make a temp folder"))?)
        .clone();
    let scratch = c
        .calibre_scratch
        .insert(
            make_temp_dir(&tmp, "method-reader-capture-calibre-")
                .map_err(io("make a temp folder"))?,
        )
        .clone();
    let real = o
        .calibre_config
        .clone()
        .unwrap_or_else(default_calibre_config);
    let calibre = copy_calibre_config(&real, &scratch).map_err(io("copy Calibre's settings"))?;

    let records = Records::default();
    let server_errors: Arc<Mutex<Vec<String>>> = Arc::default();
    let handler: Arc<OnceLock<Handler>> = Arc::default();
    let server = {
        let handler = Arc::clone(&handler);
        let server_errors = Arc::clone(&server_errors);
        // A handler that fails answers a plain 500 and is written down, so
        // the capture fails at once and names it, rather than the page
        // sitting there until the capture times out saying nothing useful.
        Server::start(move |req| match handler.get() {
            None => Response::say("", 503),
            Some(h) => h.handle(&req).unwrap_or_else(|e| {
                if let Ok(mut l) = server_errors.lock() {
                    l.push(e);
                }
                Response::say("", 500)
            }),
        })
        .map_err(io("start the page server"))?
    };
    let host = format!("127.0.0.1:{}", server.port());
    c.server = Some(server);
    let _ = handler.set(Handler::new(HandlerOptions {
        repo_dir: o.repo_dir.clone(),
        pass_one: pass_one.clone(),
        host: host.clone(),
        records: records.clone(),
    }));
    let env = engine_env(&o.library, &calibre);
    let base = format!("http://{host}");
    let interrupt = o.interrupt.clone();
    let failure = || {
        if let Some(sig) = interrupt
            .as_ref()
            .map(|i| i.load(Ordering::SeqCst))
            .filter(|s| *s != 0)
        {
            return Some(format!("interrupted by {}", signal_name(sig)));
        }
        let errs = snapshot(&server_errors);
        (!errs.is_empty()).then(|| format!("the page server failed: {}", errs.join("; ")))
    };

    let browser = c.browser.insert(launch().map_err(RunError::other)?);
    // Once a run, so a release can tell "Chrome changed" from "the window did".
    let version = browser.version();
    log(&format!("chrome  {version}"));

    let mut transcript: Option<Vec<Exchange>> = None;
    for shot in &o.shots {
        for &theme in shot.themes {
            // One capture's checks: fail at once on a server error, and if
            // it times out, name the files the server could not find for it.
            let since = snapshot(&records.not_found).len();
            let explain = || {
                let mut all = snapshot(&records.not_found);
                let mine = all.split_off(since.min(all.len()));
                if mine.is_empty() {
                    String::new()
                } else {
                    format!("the page server could not find {}", mine.join(", "))
                }
            };
            // Pass two of a framed shot: its first-pass picture, kept in
            // this run's scratch folder, placed in frame.html's window.
            let frame = |browser: &mut Box<dyn Browser>, one: &[u8]| {
                let name = format!("{}-{}.png", shot.name, theme.word());
                std::fs::write(pass_one.join(&name), one)
                    .map_err(io("keep a first-pass picture"))?;
                let (w, h) = framed_size(shot);
                let mut opts = CaptureOptions::page(
                    format!(
                        "{base}/tools/capture/frame.html?img=/pass-one/{name}&w={}&h={}&title={}",
                        shot.width,
                        shot.height,
                        shot.title.replace(' ', "%20")
                    ),
                    w,
                    h,
                    theme,
                );
                opts.transparent = true;
                opts.failure = Some(&failure);
                opts.explain = Some(&explain);
                browser.capture(&opts).map_err(RunError::other)
            };
            let png = match shot.kind {
                Kind::Site => {
                    let mut opts = CaptureOptions::page(
                        format!("{base}/tools/capture/compare.html"),
                        shot.width,
                        shot.height,
                        theme,
                    );
                    opts.failure = Some(&failure);
                    opts.explain = Some(&explain);
                    browser.capture(&opts).map_err(RunError::other)?
                }
                Kind::Window => {
                    // Every window shot starts from the same empty library,
                    // whatever shot ran before it, with an empty folder for
                    // the volumes it looks for readers in.
                    empty_library(&o.library).map_err(io("empty the library"))?;
                    std::fs::create_dir_all(o.library.join(".no-volumes"))
                        .map_err(io("make the empty volumes folder"))?;
                    let one = window
                        .pass_one(PassOne {
                            shot,
                            theme,
                            demo_pdf: &o.demo_pdf,
                            env: &env,
                            scratch: &pass_one,
                            failure: &failure,
                        })
                        .map_err(RunError::other)?;
                    frame(browser, &one)?
                }
                Kind::Terminal => {
                    // The command line runs once, from an empty library, and
                    // both themes draw what it printed.
                    if transcript.is_none() {
                        empty_library(&o.library).map_err(io("empty the library"))?;
                        std::fs::create_dir_all(o.library.join(".no-volumes"))
                            .map_err(io("make the empty volumes folder"))?;
                        transcript = Some(
                            cli.transcript(Terminal {
                                demo_pdf: &o.demo_pdf,
                                library: &o.library,
                                env: &env,
                                scratch: &pass_one,
                                failure: &failure,
                            })
                            .map_err(RunError::other)?,
                        );
                    }
                    let t = transcript.as_deref().unwrap_or_default();
                    let mut opts = CaptureOptions::page(
                        format!("{base}/tools/capture/terminal.html?t={}", terminal_query(t)),
                        shot.width,
                        shot.content_height(),
                        theme,
                    );
                    opts.failure = Some(&failure);
                    opts.explain = Some(&explain);
                    let one = browser.capture(&opts).map_err(RunError::other)?;
                    frame(browser, &one)?
                }
            };
            for out in outputs_for(shot, theme) {
                pictures.push((out, png.clone()));
            }
        }
    }

    if let Some(f) = failure().filter(|f| f.starts_with("interrupted")) {
        return Err(RunError::expected(format!("capture: {f}")));
    }
    let errs = snapshot(&server_errors);
    if !errs.is_empty() {
        return Err(RunError::expected(format!(
            "capture: the page server failed:\n  {}",
            errs.join("\n  ")
        )));
    }
    Ok(())
}
