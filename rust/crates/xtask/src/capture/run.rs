//! One capture run, from claiming the demo library to putting it back (was
//! tools/capture/run.ts).
//!
//! Everything a run creates, it removes: any engine still running, Chrome,
//! the page server, the demo library, the first-pass pictures, the copy of
//! Calibre's settings, and the folders above the library that it had to
//! make (and only those, and only if they are empty). Cleanup runs whether
//! the run succeeds, fails or is interrupted, each step on its own so one
//! failure cannot skip the rest, and a cleanup failure is reported after
//! the error that ended the run, never instead of it.
//!
//! Nothing is written to the repository until every picture has been taken,
//! no engine call was refused or failed, and the page server hit no error,
//! so a run that dies half-way leaves the committed pictures exactly as
//! they were. The library's marker names the run's pid, so two runs at once
//! cannot delete each other's library.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::backend::{Browser, CaptureOptions, PassOne, WindowBackend};
use super::cdp::{make_temp_dir, stop};
use super::http::{Response, Server};
use super::server::{EngineResult, Handler, HandlerOptions, Records, RunEngine, snapshot};
use super::shots::{Kind, Shot, framed_size, outputs_for, write_if_changed};
use crate::calibre_config::{copy_calibre_config, this_host_real_config};

/// The file that says a library is the capture tool's. Without it, a
/// library found at the demo path belongs to somebody and is never touched.
pub const MARKER: &str = ".screepub-capture";

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
    /// What draws pass one of a window shot: [`super::backend::DesktopUi`].
    pub window: Box<dyn WindowBackend>,
    /// The engine's command line, before the window's own arguments.
    pub engine: Vec<String>,
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
        Command::new("tasklist")
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

/// The environment every engine call gets, on top of this process's.
///
/// - SCREEPUB_LIBRARY: the scratch library.
/// - SCREEPUB_CONFIG_DIR: a folder under the scratch library, cleared with
///   it before every shot, which reads back as no settings at all, so the
///   pictures never drift with whatever the developer last chose in their
///   own app settings. The only write the gate lets into it is a card's
///   reader device.
/// - SCREEPUB_VOLUME_ROOTS, SCREEPUB_REMARKABLE_ENDPOINT: no real reader,
///   ever: an empty mount parent, and a reMarkable address nothing listens on.
/// - CALIBRE_CONFIG_DIRECTORY: the result screen's routes and Kindle-file
///   check probe Calibre and the KFX plugin, and Calibre run against its
///   real settings folder rewrites files there (gui.json, 0.7.3 QA). A
///   scratch COPY made for this run: the plugin stays visible, so the
///   pictures say what this machine would, and the real folder is never
///   handed to anything.
pub fn engine_env(library: &Path, calibre: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("SCREEPUB_LIBRARY", library.to_path_buf()),
        ("SCREEPUB_CONFIG_DIR", library.join(".capture-app-settings")),
        ("SCREEPUB_VOLUME_ROOTS", library.join(".no-volumes")),
        (
            "SCREEPUB_REMARKABLE_ENDPOINT",
            PathBuf::from("http://127.0.0.1:9"),
        ),
        ("CALIBRE_CONFIG_DIRECTORY", calibre.to_path_buf()),
    ]
}

/// The engines running now, by a number of the run's own.
#[derive(Clone, Default)]
struct Running {
    children: Arc<Mutex<HashMap<u64, Child>>>,
    next: Arc<AtomicU64>,
}

impl Running {
    fn is_empty(&self) -> bool {
        self.children.lock().map(|c| c.is_empty()).unwrap_or(true)
    }

    /// Wait for the engine calls in flight to finish. A call can outlive the
    /// page that made it (the version check at boot, say), and the library
    /// must not be emptied under it.
    fn settle(&self, within: Duration) -> Result<(), RunError> {
        let start = Instant::now();
        while !self.is_empty() {
            if start.elapsed() > within {
                return Err(RunError::expected(format!(
                    "capture: an engine call was still running after {} s",
                    within.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    /// Stop every engine still running and wait for it to exit: an engine
    /// mid-conversion would otherwise make the library again after it is gone.
    fn stop_all(&self) {
        let all: Vec<Child> = match self.children.lock() {
            Ok(mut c) => c.drain().map(|(_, ch)| ch).collect(),
            Err(_) => Vec::new(),
        };
        let threads: Vec<_> = all
            .into_iter()
            .map(|mut ch| std::thread::spawn(move || stop(&mut ch)))
            .collect();
        for t in threads {
            let _ = t.join();
        }
    }
}

fn engine_runner(
    engine: Vec<String>,
    library: PathBuf,
    calibre: PathBuf,
    running: Running,
    closing: Arc<AtomicBool>,
) -> RunEngine {
    let refuse = |stderr: String| EngineResult {
        code: None,
        stdout: String::new(),
        stderr,
    };
    Arc::new(move |args: &[String]| {
        // Once cleanup has begun, no engine starts: cleanup stops the engines
        // it knows about, and one started after that would outlive it and
        // make the library again. Answered like an engine that could not
        // start, which the server turns into a 500.
        if closing.load(Ordering::SeqCst) {
            return refuse("capture: cleaning up, so no new engine calls".into());
        }
        // Made before every call, not once: the library is emptied between
        // shots. A missing root lists nothing too, but an empty folder is
        // the arrangement the tests pin.
        let no_volumes = library.join(".no-volumes");
        if let Err(e) = std::fs::create_dir_all(&no_volumes) {
            return refuse(format!(
                "capture: could not make {}: {e}",
                no_volumes.display()
            ));
        }
        let Some((program, rest)) = engine.split_first() else {
            return refuse("capture: no engine to run".into());
        };
        let mut cmd = Command::new(program);
        cmd.args(rest)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in engine_env(&library, &calibre) {
            cmd.env(k, v);
        }
        // Spawned under the lock, so cleanup either sees this engine or has
        // already said no more engines.
        let (id, out, err) = {
            let Ok(mut children) = running.children.lock() else {
                return refuse("capture: the engine list is poisoned".into());
            };
            if closing.load(Ordering::SeqCst) {
                return refuse("capture: cleaning up, so no new engine calls".into());
            }
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => return refuse(format!("capture: could not start the engine: {e}")),
            };
            let out = child.stdout.take();
            let err = child.stderr.take();
            let id = running.next.fetch_add(1, Ordering::SeqCst);
            children.insert(id, child);
            (id, out, err)
        };
        // Both streams at once. A pipe nobody reads fills up, and an engine
        // blocked writing to it never exits.
        let drain = |pipe: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut s = Vec::new();
                if let Some(mut p) = pipe {
                    let _ = p.read_to_end(&mut s);
                }
                String::from_utf8_lossy(&s).into_owned()
            })
        };
        let out = drain(out.map(|p| Box::new(p) as Box<dyn Read + Send>));
        let err = drain(err.map(|p| Box::new(p) as Box<dyn Read + Send>));
        let code = loop {
            let status = match running.children.lock() {
                Ok(mut c) => match c.get_mut(&id) {
                    // Taken by cleanup, which stops it: killed.
                    None => break None,
                    Some(ch) => match ch.try_wait() {
                        Ok(Some(st)) => {
                            c.remove(&id);
                            Some(st)
                        }
                        Ok(None) => None,
                        Err(_) => {
                            c.remove(&id);
                            break None;
                        }
                    },
                },
                Err(_) => break None,
            };
            if let Some(st) = status {
                break st.code();
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        EngineResult {
            code,
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
        }
    })
}

/// The run's cleanup: every step on its own, in this order, because nothing
/// may write into the library after it is removed.
struct Cleanup<'a> {
    o: &'a RunOptions,
    top: PathBuf,
    closing: Arc<AtomicBool>,
    running: Running,
    browser: Option<Box<dyn Browser>>,
    server: Option<Server>,
    pass_one: Option<PathBuf>,
    calibre_scratch: Option<PathBuf>,
}

impl Cleanup<'_> {
    fn run(&mut self) -> Vec<String> {
        // First: no engine may start from here on.
        self.closing.store(true, Ordering::SeqCst);
        let mut errors = Vec::new();
        let mut step = |what: &str, r: Result<(), String>| {
            if let Err(e) = r {
                errors.push(format!("capture: cleanup could not {what}: {e}"));
            }
        };
        self.running.stop_all();
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

/// Takes every picture, then writes them all, or none.
pub fn run_capture(mut o: RunOptions) -> Result<(), RunError> {
    let launch = std::mem::replace(&mut o.launch, Box::new(|| Err("already launched".into())));
    let mut window = std::mem::replace(&mut o.window, Box::new(super::backend::DesktopUi));
    let mut log = std::mem::replace(&mut o.log, Box::new(|_| {}));
    let top = claim_library(&o.library, std::process::id())?;
    let closing = Arc::new(AtomicBool::new(false));
    let running = Running::default();
    let mut c = Cleanup {
        o: &o,
        top,
        closing: Arc::clone(&closing),
        running: running.clone(),
        browser: None,
        server: None,
        pass_one: None,
        calibre_scratch: None,
    };
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let result = take(&mut c, window.as_mut(), log.as_mut(), &mut pictures, launch);
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
            // Only now: every picture taken, nothing refused, nothing failed.
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
        .insert(make_temp_dir(&tmp, "screepub-capture-").map_err(io("make a temp folder"))?)
        .clone();
    let scratch = c
        .calibre_scratch
        .insert(make_temp_dir(&tmp, "screepub-capture-calibre-").map_err(io("make a temp folder"))?)
        .clone();
    let real = o
        .calibre_config
        .clone()
        .unwrap_or_else(default_calibre_config);
    let calibre = copy_calibre_config(&real, &scratch).map_err(io("copy Calibre's settings"))?;

    let token = super::random_token();
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
        token,
        library: o.library.clone(),
        demo_pdf: o.demo_pdf.clone(),
        host: host.clone(),
        run_engine: engine_runner(
            o.engine.clone(),
            o.library.clone(),
            calibre,
            c.running.clone(),
            Arc::clone(&c.closing),
        ),
        records: records.clone(),
    }));
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
            let png = match shot.kind {
                Kind::Site => {
                    let mut opts = CaptureOptions::page(
                        format!("{base}/tools/capture/hero.html"),
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
                    // whatever shot ran before it.
                    c.running.settle(Duration::from_secs(60))?;
                    empty_library(&o.library).map_err(io("empty the library"))?;
                    let one = window
                        .pass_one(PassOne {
                            shot,
                            theme,
                            base: &base,
                            browser: browser.as_mut(),
                            failure: &failure,
                            explain: &explain,
                        })
                        .map_err(RunError::other)?;
                    let name = format!("{}-{}.png", shot.name, theme.word());
                    std::fs::write(pass_one.join(&name), &one)
                        .map_err(io("keep a first-pass picture"))?;
                    let (w, h) = framed_size(shot);
                    let mut opts = CaptureOptions::page(
                        format!(
                            "{base}/tools/capture/frame.html?img=/pass-one/{name}&w={}&h={}",
                            shot.width, shot.height
                        ),
                        w,
                        h,
                        theme,
                    );
                    opts.transparent = true;
                    opts.failure = Some(&failure);
                    opts.explain = Some(&explain);
                    browser.capture(&opts).map_err(RunError::other)?
                }
            };
            for out in outputs_for(shot, theme) {
                pictures.push((out, png.clone()));
            }
        }
    }

    c.running.settle(Duration::from_secs(60))?;
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
    let refused = snapshot(&records.refused);
    if !refused.is_empty() {
        return Err(RunError::expected(format!(
            "capture: the window made engine calls the gate refused:\n  {}",
            refused.join("\n  ")
        )));
    }
    let failed = snapshot(&records.failed);
    if !failed.is_empty() {
        return Err(RunError::expected(format!(
            "capture: engine calls failed:\n  {}",
            failed.join("\n  ")
        )));
    }
    Ok(())
}
