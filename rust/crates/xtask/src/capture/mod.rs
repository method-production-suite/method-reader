//! The README and site pictures: was tools/capture-screens.ts,
//! tools/review-screens.ts, tools/capture/*.ts and tools/make-og.sh.
//!
//! - `cargo xtask capture-screens [--only <shot>]` takes every README and
//!   site picture from the real window running the real engine on
//!   tests/fixtures/field-station.pdf (spec
//!   docs/superpowers/specs/2026-09-22-readme-site-screens-design.md, part
//!   3, and the Method suite's four shots, [`shots`]), and writes
//!   assets/screens/<shot>-<theme>.png and the same file into site/img/. A
//!   file is rewritten only when its bytes change. Prints Chrome's version, then one line per file: written or
//!   unchanged. Nothing is written unless every picture was taken.
//! - `cargo xtask review-screens --out <page.html> [--repo <dir>]`: which
//!   pictures changed since the last commit, old beside new.
//! - `cargo xtask make-og` renders site/og-card.html to site/og.png at
//!   1200 by 630, the size every social unfurler expects.
//!
//! Each window picture is taken twice over. Pass one is the bare window:
//! the real window (rust/crates/sp-app), drawn offscreen by egui through
//! wgpu by sp-app's capture program (examples/capture.rs), which this
//! command builds first and runs once per picture with the run's own
//! scratch library and settings. Pass two is that picture in its frame
//! and shadow, drawn by Chrome from tools/capture/frame.html; Chrome also
//! draws the site's before and after from tools/capture/compare.html. The
//! terminal shot is framed the same way: its pass one is
//! tools/capture/terminal.html drawing what the real `method-reader` binary
//! (sp-cli, built first too) printed converting the demo script into the
//! scratch library. [`backend`] says what the window program and the
//! command line do and do not do.
//!
//! The pieces: [`shots`] (the list and where each goes), [`backend`] (the
//! window program and Chrome, behind two traits), [`server`] and [`http`]
//! (the page server Chrome reads the frame, the before and after, the terminal and pass one from, on
//! threads of its own), [`cdp`] (Chrome), [`run`] (one whole run and its
//! cleanup), [`review`] and [`picture`].
//!
//! capture-screens needs Chrome at [`cdp::CHROME`], and macOS: the demo
//! library is /Users/Shared/Documents/Method Reader, because the result screen
//! prints the book's full path and that folder reads naturally with no
//! username in it. The tool marks the library as its own, refuses to run if
//! an unmarked one is already there, and removes it when done, along with
//! any folder above it that it had to create ([`run`] has the details).
//!
//! Exit codes: 0 done. 1 failed: if the window program would not build, a
//! picture could not be taken (the window refused an engine call of its
//! own, say), or the page server hit an error, nothing was written; if only the cleanup afterwards failed, the pictures
//! WERE written and the message says what could not be cleaned up. 2 no
//! Chrome, so nothing was retaken (a release warns about that rather than
//! stopping). 130 or 143 interrupted, after cleaning up; nothing written.

pub mod backend;
pub mod cdp;
pub mod http;
pub mod picture;
pub mod review;
pub mod run;
pub mod server;
pub mod shots;

use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

use backend::{Browser, CaptureOptions, EguiWindow, RealCli, Wait};
use run::{RunOptions, run_capture};
use shots::{SHOTS, Theme};

use crate::paths::{repo_root, workspace_root};

const LIBRARY: &str = "/Users/Shared/Documents/Method Reader";

/// 32 hex digits no other process can guess: SipHash under the keys std
/// seeds from the OS's randomness, over the time and a counter.
pub fn random_token() -> String {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut out = String::new();
    for half in 0..2u8 {
        let mut h = std::hash::RandomState::new().build_hasher();
        h.write_u128(now);
        h.write_usize(n);
        h.write_u8(half);
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// `--name value` and `--name=value` for the names given, as Node's
/// parseArgs reads them; anything else is refused.
pub fn parse_flags(args: &[String], names: &[&str]) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        let Some(flag) = a.strip_prefix("--") else {
            return Err(format!("Unexpected argument '{a}'"));
        };
        let (name, value) = match flag.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (flag, None),
        };
        if !names.contains(&name) {
            return Err(format!("Unknown option '--{name}'"));
        }
        let value = match value {
            Some(v) => v,
            None => {
                let v = args
                    .get(i)
                    .ok_or_else(|| format!("Option '--{name} <value>' argument missing"))?;
                i += 1;
                v.clone()
            }
        };
        out.insert(name.to_string(), value);
    }
    Ok(out)
}

/// `cargo xtask capture-screens`: its exit code.
pub fn capture_main(args: &[String]) -> u8 {
    let flags = match parse_flags(args, &["only"]) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("capture: {e}");
            return 1;
        }
    };
    let shots: Vec<_> = match flags.get("only") {
        None => SHOTS.to_vec(),
        Some(only) => SHOTS.iter().filter(|s| s.name == only).copied().collect(),
    };
    if shots.is_empty() {
        eprintln!("capture: no shot named {}", flags["only"]);
        return 1;
    }
    let chrome = PathBuf::from(cdp::CHROME);
    if !chrome.exists() {
        eprintln!(
            "capture: no Chrome at {}. Pictures NOT retaken.",
            cdp::CHROME
        );
        return 2;
    }

    // A first signal stops the run at its next look, which cleans up and
    // answers 130 or 143; a second means "now", cleanup or not.
    let signal = Arc::new(AtomicUsize::new(0));
    let once = Arc::new(AtomicBool::new(false));
    for (sig, code) in [
        (signal_hook::consts::SIGINT, 130),
        (signal_hook::consts::SIGTERM, 143),
    ] {
        let _ = signal_hook::flag::register_conditional_shutdown(sig, code, Arc::clone(&once));
        let _ = signal_hook::flag::register(sig, Arc::clone(&once));
        let _ = signal_hook::flag::register_usize(sig, Arc::clone(&signal), sig as usize);
    }

    let (window, cli) = match build_window().and_then(|w| Ok((w, build_cli()?))) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("capture: {e}");
            return 1;
        }
    };
    let repo = repo_root();
    let result = run_capture(RunOptions {
        shots,
        demo_pdf: repo
            .join("tests")
            .join("fixtures")
            .join("field-station.pdf"),
        library: PathBuf::from(LIBRARY),
        launch: Box::new(move || cdp::launch(&chrome).map(|c| Box::new(c) as Box<dyn Browser>)),
        window: Box::new(EguiWindow::new(&window)),
        cli: Box::new(RealCli::new(&cli)),
        out_dir: repo.clone(),
        repo_dir: repo,
        tmp: None,
        calibre_config: None,
        log: Box::new(|line| println!("{line}")),
        interrupt: Some(signal),
    });
    match result {
        Ok(()) => 0,
        Err(e) => {
            if let Some(sig) = e.interrupted {
                eprintln!(
                    "capture: {}, cleaned up",
                    if sig == 2 { "SIGINT" } else { "SIGTERM" }
                );
            } else {
                eprintln!("{}", e.message);
            }
            for c in &e.cleanup {
                eprintln!("{c}");
            }
            match e.interrupted {
                Some(2) => 130,
                Some(_) => 143,
                None => 1,
            }
        }
    }
}

/// Where cargo puts this workspace's release builds: CARGO_TARGET_DIR when
/// it is set (relative to the workspace, as cargo reads it), rust/target
/// otherwise.
pub fn release_dir() -> PathBuf {
    let ws = workspace_root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .filter(|t| !t.is_empty())
        .map_or_else(|| ws.join("target"), |t| ws.join(t));
    target.join("release")
}

/// Builds sp-app's capture program in the release profile, the one the
/// shipped window is built in, and answers where it is.
pub fn build_window() -> Result<PathBuf, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = std::process::Command::new(cargo)
        .args([
            "build",
            "--quiet",
            "--release",
            "--package",
            "sp-app",
            "--example",
            "capture",
        ])
        .current_dir(workspace_root())
        .status()
        .map_err(|e| format!("could not build the window program: {e}"))?;
    if !status.success() {
        return Err("the window program (sp-app's capture example) did not build".into());
    }
    let exe = if cfg!(windows) {
        "capture.exe"
    } else {
        "capture"
    };
    Ok(release_dir().join("examples").join(exe))
}

/// Builds the `method-reader` command line in the release profile, the one
/// it ships in, for the terminal shot, and answers where it is.
pub fn build_cli() -> Result<PathBuf, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = std::process::Command::new(cargo)
        .args([
            "build",
            "--quiet",
            "--release",
            "--package",
            "sp-cli",
            "--bin",
            "method-reader",
        ])
        .current_dir(workspace_root())
        .status()
        .map_err(|e| format!("could not build the command line: {e}"))?;
    if !status.success() {
        return Err("the command line (sp-cli's method-reader) did not build".into());
    }
    let exe = if cfg!(windows) {
        "method-reader.exe"
    } else {
        "method-reader"
    };
    Ok(release_dir().join(exe))
}

/// `cargo xtask review-screens`: its exit code.
pub fn review_main(args: &[String]) -> u8 {
    review::main(args, &repo_root())
}

/// A file:// URL for an absolute path, every byte outside the unreserved
/// set and `/` percent-encoded.
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `cargo xtask make-og`: renders site/og-card.html to site/og.png at
/// 1200x630 on a transparent background, then prints its size the way
/// macOS's `sips -g pixelWidth -g pixelHeight` did. Re-run after editing
/// og-card.html. Exit codes: 0 done, 1 no Chrome or failed.
pub fn make_og_main(_args: &[String]) -> u8 {
    let chrome = Path::new(cdp::CHROME);
    if !chrome.exists() {
        eprintln!("Google Chrome not found at {}", cdp::CHROME);
        return 1;
    }
    let site = repo_root().join("site");
    match make_og(chrome, &site.join("og-card.html"), &site.join("og.png")) {
        Ok((w, h)) => {
            println!(
                "{}\n  pixelWidth: {w}\n  pixelHeight: {h}",
                site.join("og.png").display()
            );
            0
        }
        Err(e) => {
            eprintln!("make-og: {e}");
            1
        }
    }
}

/// Draws `card` with the Chrome at `chrome` into `out`; answers its size.
pub fn make_og(chrome: &Path, card: &Path, out: &Path) -> Result<(u32, u32), String> {
    let mut browser = cdp::launch(chrome)?;
    let shot = browser.capture(&CaptureOptions {
        transparent: true,
        scale: 1.0,
        wait: Wait::Loaded,
        timeout: Duration::from_secs(60),
        ..CaptureOptions::page(file_url(card), 1200, 630, Theme::Light)
    });
    let closed = browser.close();
    let png = shot?;
    closed?;
    std::fs::write(out, &png).map_err(|e| format!("could not write {}: {e}", out.display()))?;
    picture::size(&png)
}
