//! What draws the pictures, behind three traits, so the run (claiming the
//! library, the shot list, framing, the all-or-nothing write, cleanup) does
//! not care what the window is made of.
//!
//! - [`Browser`] draws web pages: the site's before and after
//!   (compare.html), the terminal shot's first pass (terminal.html), and
//!   pass two of every framed picture (frame.html). Chrome over the
//!   DevTools protocol ([`super::cdp`]). The pages are HTML and stay HTML.
//! - [`CommandLine`] runs the real `method-reader` on the demo script for
//!   the terminal shot and says what a terminal showed ([`RealCli`]).
//! - [`WindowBackend`] draws pass one: the bare window, in one shot's
//!   state, at the shot's size, in one theme, as PNG bytes at twice the
//!   window's size in points (a 1280 by 800 window is a 2560 by 1600
//!   picture, which is what frame.html expects).
//!
//! The window backend is [`EguiWindow`]: the real window, rust/crates/
//! sp-app, drawn offscreen by egui through wgpu. It runs sp-app's capture
//! program (examples/capture.rs, built by `cargo xtask capture-screens`
//! before the run) once per picture, as a process of its own, because the
//! engine linked into it reads the process environment, and that
//! environment is the run's own ([`super::run::engine_env`]: the scratch
//! library, scratch app settings, no volumes, a reMarkable address nothing
//! listens on, a scratch COPY of Calibre's settings). The program:
//!
//! - draws `shot.width` by `shot.content_height()` points (the shot less
//!   the title bar frame.html draws above it) at two pixels a point;
//! - forces light or dark, whatever the OS says;
//! - reaches the shot as a person would: `drop` is the idle well; `result`
//!   is the demo PDF dropped on it and converted into the scratch library,
//!   the send panel's device cards beside the book's first page; `read` is
//!   the same book on the Read tab, its scene index and script drawn;
//! - waits until nothing moves, then draws with the version stamp and the
//!   library line hidden, so a picture changes only when the window's look
//!   does;
//! - converts no file but the demo, reads books only from the scratch
//!   library, answers the two Calibre questions as a machine without
//!   Calibre would, and refuses (and fails on) anything that copies, sends,
//!   builds, ejects, exports or installs;
//! - writes the picture's raw RGBA, which [`super::picture::encode_rgba`]
//!   turns into PNG bytes here, deterministically: same pixels, same bytes,
//!   so `write_if_changed` keeps an unchanged picture unchanged.
//!
//! sp-app/src/capture.rs holds those rules and its tests hold it to them;
//! this side builds the program's orders ([`window_args`]), runs it with
//! the run's environment, stops it if the run is interrupted or it takes
//! too long, and checks that what it wrote is the size it should be.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::picture::encode_rgba;
use super::shots::{Shot, Theme};

/// When a page is ready to be photographed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// The page sets `window.__captureState` to `ready` or `failed` (the
    /// capture's own pages: compare.html, terminal.html, frame.html).
    PageSaysReady,
    /// A page that says nothing (site/og-card.html): loaded, fonts too.
    Loaded,
}

pub struct CaptureOptions<'a> {
    pub url: String,
    /// CSS pixels.
    pub width: u32,
    pub height: u32,
    pub theme: Theme,
    pub transparent: bool,
    /// Device pixels per CSS pixel: 2 for the README, 1 for the social card.
    pub scale: f64,
    pub wait: Wait,
    /// Asked on every poll: a sentence here fails the capture at once, the
    /// way a page server that has already failed should, rather than after
    /// the whole wait.
    pub failure: Option<&'a dyn Fn() -> Option<String>>,
    /// Added to a timeout's message: what might explain the page never
    /// saying ready (the files the server could not find, say).
    pub explain: Option<&'a dyn Fn() -> String>,
    pub timeout: Duration,
}

impl CaptureOptions<'_> {
    /// A capture of `url` at the README's scale, waiting on the page.
    pub fn page(url: String, width: u32, height: u32, theme: Theme) -> CaptureOptions<'static> {
        CaptureOptions {
            url,
            width,
            height,
            theme,
            transparent: false,
            scale: 2.0,
            wait: Wait::PageSaysReady,
            failure: None,
            explain: None,
            timeout: super::cdp::CAPTURE_TIMEOUT,
        }
    }
}

/// Something that draws web pages to PNG.
pub trait Browser {
    /// What it calls itself, e.g. Chrome/153.0.8010.53.
    fn version(&self) -> String;
    fn capture(&mut self, o: &CaptureOptions<'_>) -> Result<Vec<u8>, String>;
    /// Safe to call more than once; later calls answer what the first did.
    fn close(&mut self) -> Result<(), String>;
}

/// What a window backend gets for one pass-one picture.
pub struct PassOne<'a> {
    pub shot: &'a Shot,
    pub theme: Theme,
    /// The one file the window may convert.
    pub demo_pdf: &'a Path,
    /// The environment the window's engine runs with, on top of this
    /// process's ([`super::run::engine_env`]).
    pub env: &'a [(&'static str, PathBuf)],
    /// A scratch folder of this run's, for the window's raw picture.
    pub scratch: &'a Path,
    /// Asked while the window works: a sentence here (an interrupt) stops
    /// it at once.
    pub failure: &'a dyn Fn() -> Option<String>,
}

/// Something that draws the bare window in one shot's state.
pub trait WindowBackend {
    fn pass_one(&mut self, p: PassOne<'_>) -> Result<Vec<u8>, String>;
}

/// The capture program's orders for one picture.
pub fn window_args(shot: &Shot, theme: Theme, demo: &Path, out: &Path) -> Vec<String> {
    vec![
        "--shot".into(),
        shot.name.into(),
        "--theme".into(),
        theme.word().into(),
        "--width".into(),
        shot.width.to_string(),
        "--height".into(),
        shot.content_height().to_string(),
        "--demo".into(),
        demo.to_string_lossy().into_owned(),
        "--out".into(),
        out.to_string_lossy().into_owned(),
    ]
}

/// Pixels per point in every window picture.
pub const SCALE: u32 = 2;

/// The longest one window picture may take: a conversion of the demo
/// script, a moment to settle, and the drawing.
pub const WINDOW_TIMEOUT: Duration = Duration::from_secs(180);

/// The real window, drawn by sp-app's capture program.
pub struct EguiWindow {
    /// The program, then any arguments before the picture's orders.
    pub program: Vec<String>,
    pub timeout: Duration,
}

impl EguiWindow {
    pub fn new(program: &Path) -> EguiWindow {
        EguiWindow {
            program: vec![program.to_string_lossy().into_owned()],
            timeout: WINDOW_TIMEOUT,
        }
    }
}

impl WindowBackend for EguiWindow {
    fn pass_one(&mut self, p: PassOne<'_>) -> Result<Vec<u8>, String> {
        let out = p
            .scratch
            .join(format!("{}-{}.rgba", p.shot.name, p.theme.word()));
        let Some((program, rest)) = self.program.split_first() else {
            return Err("capture: no window program to run".into());
        };
        let mut cmd = Command::new(program);
        cmd.args(rest)
            .args(window_args(p.shot, p.theme, p.demo_pdf, &out))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        for (k, v) in p.env {
            cmd.env(k, v);
        }
        let what = format!("the {} window ({})", p.shot.name, p.theme.word());
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("capture: could not start {what}: {e}"))?;
        // stderr is read on a thread of its own: a pipe nobody reads fills
        // up, and a program blocked writing to it never exits.
        let err = child.stderr.take();
        let said = std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut e) = err {
                let _ = std::io::Read::read_to_string(&mut e, &mut s);
            }
            s
        });
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(st)) => break st,
                Ok(None) => {}
                Err(e) => {
                    super::cdp::stop(&mut child);
                    return Err(format!("capture: lost {what}: {e}"));
                }
            }
            if let Some(f) = (p.failure)() {
                super::cdp::stop(&mut child);
                return Err(format!("capture: {what}: {f}"));
            }
            if start.elapsed() > self.timeout {
                super::cdp::stop(&mut child);
                return Err(format!(
                    "capture: {what} timed out after {} s",
                    self.timeout.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let said = said.join().unwrap_or_default();
        if !status.success() {
            let code = status
                .code()
                .map_or_else(|| "a signal".to_string(), |c| c.to_string());
            return Err(format!(
                "capture: {what} exited with {code}: {}",
                said.trim()
            ));
        }
        let pixels =
            std::fs::read(&out).map_err(|e| format!("capture: {what} wrote no picture: {e}"))?;
        let _ = std::fs::remove_file(&out);
        let (w, h) = (p.shot.width * SCALE, p.shot.content_height() * SCALE);
        encode_rgba(w, h, &pixels).map_err(|e| format!("capture: {what}: {e}"))
    }
}

/// One command in the terminal shot: what was typed, and what it printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub command: String,
    pub output: String,
}

/// What a command-line backend gets for the terminal shot.
pub struct Terminal<'a> {
    /// The one file it may convert.
    pub demo_pdf: &'a Path,
    /// The demo library the conversion writes into.
    pub library: &'a Path,
    /// The environment the engine runs with ([`super::run::engine_env`]).
    pub env: &'a [(&'static str, PathBuf)],
    /// A scratch folder of this run's: the script is converted from a copy
    /// in a folder of its own here, so the command can name it plainly.
    pub scratch: &'a Path,
    /// Asked while the command runs: a sentence here stops it at once.
    pub failure: &'a dyn Fn() -> Option<String>,
}

/// Something that runs the command line on the demo script and says what a
/// terminal showed.
pub trait CommandLine {
    fn transcript(&mut self, t: Terminal<'_>) -> Result<Vec<Exchange>, String>;
}

/// How many lines of the .fountain the terminal shot shows.
pub const HEAD_LINES: usize = 18;

/// The real `method-reader` binary (sp-cli, built by `cargo xtask
/// capture-screens` before the run), run once on a copy of the demo script
/// with `--library`, so it writes into the run's scratch library under the
/// run's scratch settings. The two commands after it are read off what it
/// wrote: the book's folder as `ls` lists it, and the head of the .fountain.
/// Every word on screen is real, and the same every run: the library's path
/// is fixed, and nothing the command prints carries a time.
pub struct RealCli {
    pub program: PathBuf,
    pub timeout: Duration,
}

impl RealCli {
    pub fn new(program: &Path) -> RealCli {
        RealCli {
            program: program.to_path_buf(),
            timeout: WINDOW_TIMEOUT,
        }
    }
}

/// `"<path>"` when the path has a space in it, as a person would type it.
fn shell_word(p: &Path) -> String {
    let s = p.to_string_lossy();
    if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s.into_owned()
    }
}

/// What a terminal showed for the command line on the demo script, given
/// what the conversion printed and the book's folder it wrote.
pub fn transcript_of(demo_name: &str, printed: &str, book: &Path) -> Result<Vec<Exchange>, String> {
    let stem = demo_name.strip_suffix(".pdf").unwrap_or(demo_name);
    let mut names: Vec<String> = std::fs::read_dir(book)
        .map_err(|e| format!("capture: no book folder at {}: {e}", book.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    let fountain = book.join(format!("{stem}.fountain"));
    let text = std::fs::read_to_string(&fountain)
        .map_err(|e| format!("capture: could not read {}: {e}", fountain.display()))?;
    let head: Vec<&str> = text.lines().take(HEAD_LINES).collect();
    Ok(vec![
        Exchange {
            command: format!("method-reader {demo_name} --library"),
            output: printed.trim_end().to_string(),
        },
        Exchange {
            command: format!("ls -1 {}", shell_word(book)),
            output: names.join("\n"),
        },
        Exchange {
            command: format!("head -n {HEAD_LINES} {}", shell_word(&fountain)),
            output: head.join("\n"),
        },
    ])
}

impl CommandLine for RealCli {
    fn transcript(&mut self, t: Terminal<'_>) -> Result<Vec<Exchange>, String> {
        let name = t
            .demo_pdf
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or("capture: the demo script has no file name")?;
        let work = t.scratch.join("cli-work");
        std::fs::create_dir_all(&work)
            .and_then(|()| std::fs::copy(t.demo_pdf, work.join(&name)).map(|_| ()))
            .map_err(|e| format!("capture: could not copy the demo script: {e}"))?;
        let mut cmd = Command::new(&self.program);
        cmd.args([name.as_str(), "--library"])
            .current_dir(&work)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in t.env {
            cmd.env(k, v);
        }
        let what = "the command line";
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("capture: could not start {what}: {e}"))?;
        let read = |p: Option<Box<dyn std::io::Read + Send>>| {
            std::thread::spawn(move || {
                let mut s = String::new();
                if let Some(mut p) = p {
                    let _ = p.read_to_string(&mut s);
                }
                s
            })
        };
        let out = read(child.stdout.take().map(|p| Box::new(p) as _));
        let err = read(child.stderr.take().map(|p| Box::new(p) as _));
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(st)) => break st,
                Ok(None) => {}
                Err(e) => {
                    super::cdp::stop(&mut child);
                    return Err(format!("capture: lost {what}: {e}"));
                }
            }
            if let Some(f) = (t.failure)() {
                super::cdp::stop(&mut child);
                return Err(format!("capture: {what}: {f}"));
            }
            if start.elapsed() > self.timeout {
                super::cdp::stop(&mut child);
                return Err(format!(
                    "capture: {what} timed out after {} s",
                    self.timeout.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let (out, err) = (
            out.join().unwrap_or_default(),
            err.join().unwrap_or_default(),
        );
        let _ = std::fs::remove_dir_all(&work);
        if !status.success() {
            return Err(format!("capture: {what} failed: {}", err.trim()));
        }
        // A clean conversion prints to stdout alone; anything on stderr (a
        // warning) is shown after it, as a terminal would show it last.
        let printed = if err.trim().is_empty() {
            out
        } else {
            format!("{}\n{}", out.trim_end(), err.trim_end())
        };
        let stem = name.strip_suffix(".pdf").unwrap_or(&name);
        transcript_of(&name, &printed, &t.library.join(stem))
    }
}

/// `?t=` for tools/capture/terminal.html: the transcript as JSON, every
/// byte outside the unreserved set percent-encoded.
pub fn terminal_query(t: &[Exchange]) -> String {
    let json = serde_json::Value::Array(
        t.iter()
            .map(|e| serde_json::json!({ "c": e.command, "o": e.output }))
            .collect(),
    )
    .to_string();
    let mut out = String::new();
    for b in json.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
