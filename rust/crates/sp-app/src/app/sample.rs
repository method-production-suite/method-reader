//! The sample's half in the window (what it decides is model/sample.rs):
//! the first launch, Help, Open Sample, and the sample's folder on disk.
//!
//! Opening the sample puts the built-in PDF in its folder (the host's
//! `sample_dir`, in the app-data folder, never the library) and converts it
//! there through the engine's ordinary conversion, or reopens the book
//! already made when its `.fountain` is current. Either way it lands on
//! Preview, as a person's own script does once they press Preview: the
//! Convert page's send panel, which looks for e-readers and builds a Kindle
//! file ahead, starts only when a person goes there. Nothing is sent and no
//! route runs.

use std::path::Path;

use serde_json::Value;

use super::App;
use crate::engine::Request;
use crate::model::sample::{self as m, OnDisk, Paths, Plan};

/// The version that converts the sample, as its stamp records it.
const RUNNING: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Default)]
pub struct State {
    /// This launch is the first (read when the window was made): the
    /// shipped window opens the sample on it ([`first_launch`]).
    pub first_launch: bool,
}

/// The sample's files, where this computer keeps them.
pub fn paths(app: &App) -> Option<Paths> {
    app.host.sample_dir.as_deref().map(Paths::in_folder)
}

/// Whether `path` is the sample's PDF.
pub fn is_sample_path(app: &App, path: &str) -> bool {
    paths(app).is_some_and(|p| p.pdf == path)
}

/// Whether the script open is the sample.
pub fn is_open(app: &App) -> bool {
    app.script
        .as_ref()
        .is_some_and(|s| is_sample_path(app, &s.path))
}

/// The first launch, from the shipped window once it is made (main.rs): the
/// sample, when this launch is the first and nothing is open. No test,
/// picture or headless run calls it unless it means to.
pub fn first_launch(app: &mut App) {
    if !std::mem::take(&mut app.sample.first_launch) {
        return;
    }
    if app.script.is_some() || app.convert.converting {
        return;
    }
    open(app);
}

/// Help, Open Sample: the sample, reopened when it is current.
pub fn open(app: &mut App) {
    start(app, false);
}

/// Convert the sample's PDF again, whatever is there ("Convert it again").
pub fn convert_again(app: &mut App) {
    start(app, true);
}

fn start(app: &mut App, fresh: bool) {
    if app.convert.converting {
        return;
    }
    let Some(p) = paths(app) else { return };
    if let Err(e) = put_pdf(&p) {
        let message = format!("The sample could not be put in {}: {e}", p.dir.display());
        super::convert::sample_failed(app, &message, &p.pdf);
        return;
    }
    let plan = if fresh {
        Plan::Convert
    } else {
        m::plan(&on_disk(&p), RUNNING)
    };
    let (input, stamp) = match plan {
        Plan::Convert => (p.pdf.clone(), None),
        Plan::Reopen { stamp } => (p.fountain.clone(), Some(stamp)),
    };
    super::convert::convert_sample(
        app,
        Request::ConvertTo {
            path: input,
            out: p.epub.clone(),
        },
        p.pdf,
        p.epub,
        stamp,
    );
}

/// The built-in PDF in the sample's folder, written only when what is
/// there differs (so the `.fountain` made from it stays no older than it).
fn put_pdf(p: &Paths) -> std::io::Result<()> {
    std::fs::create_dir_all(&p.dir)?;
    if std::fs::read(&p.pdf).is_ok_and(|bytes| bytes == m::PDF) {
        return Ok(());
    }
    sp_fsutil::write_file_atomic(Path::new(&p.pdf), m::PDF).map_err(std::io::Error::other)
}

fn on_disk(p: &Paths) -> OnDisk {
    let modified = |path: &str| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let fountain_fresh = match (modified(&p.fountain), modified(&p.pdf)) {
        (Some(fountain), Some(pdf)) => fountain >= pdf,
        _ => false,
    };
    OnDisk {
        pdf_is_bundled: std::fs::read(&p.pdf).is_ok_and(|bytes| bytes == m::PDF),
        book: Path::new(&p.epub).is_file(),
        fountain_fresh,
        stamp: std::fs::read_to_string(&p.stamp)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok()),
    }
}

/// The sample converted: keep what the conversion said, for a reopen. A
/// stamp that cannot be written only means the next open converts again.
pub fn converted(app: &App, answer: &Value) {
    if let Some(p) = paths(app) {
        let stamp = m::stamp_from(answer, RUNNING);
        let _ = sp_fsutil::write_file_atomic(Path::new(&p.stamp), stamp.to_string());
    }
}
