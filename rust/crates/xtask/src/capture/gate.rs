//! Which engine calls the capture tool may make on the window's behalf
//! (was tools/capture/gate.ts).
//!
//! The gate allows only calls whose WHOLE SHAPE the window's own argv
//! builders reproduce exactly, not calls that merely start with a
//! recognised verb, or merely put -o somewhere inside the library. That is
//! the difference between a real boundary and a checklist: an unknown flag,
//! a repeated flag, a `--flag=value` form the window never writes, a
//! missing constant flag like --library, or a relative path all fail
//! closed, because none of them is a shape any builder can produce. The
//! gate rebuilds the call with the same builder and compares; a new
//! VARIABLE part (a flag that takes a path, say) is refused until the gate
//! is taught to recognise it, because nothing here can guess whether an
//! unfamiliar value is safe.
//!
//! The builders are [`super::window_argv`], a copy of desktop/ui/app.js's
//! held to the original by the tests (see that module).
//!
//! Besides the conversion itself it allows the result screen's reads: the
//! devices poll, the demo book's routes and Kindle check, and the one write
//! a card makes (the reader's device, into the capture's scratch config).
//! Every one of them lists or reads; nothing that copies, sends, builds or
//! ejects gets through, and run.rs pins the device seams at empty places.
//! Pure.

use std::path::{Component, Path, PathBuf};

use super::window_argv as argv;

pub struct GateContext<'a> {
    /// The one file the capture may convert.
    pub demo_pdf: &'a Path,
    /// The scratch library the engine writes into for the capture.
    pub library: &'a Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateAnswer {
    Allow,
    Refuse(String),
}

impl GateAnswer {
    pub fn allowed(&self) -> bool {
        matches!(self, GateAnswer::Allow)
    }
}

/// `path` with `.` and `..` folded away lexically, as Node's resolve does
/// for an absolute path.
pub fn resolve(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never above the root: `/..` is `/`.
                if out.parent().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn inside(path: Option<&str>, dir: &Path) -> bool {
    let Some(path) = path.filter(|p| !p.is_empty()) else {
        return false;
    };
    let path = Path::new(path);
    if !path.is_absolute() {
        return false;
    }
    let (p, d) = (resolve(path), resolve(dir));
    p != d && p.starts_with(&d)
}

fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

fn same(a: &[String], b: &[String]) -> bool {
    a == b
}

pub fn gate_engine_call(args: &[String], ctx: &GateContext<'_>) -> GateAnswer {
    let Some(first) = args.first().map(String::as_str) else {
        return GateAnswer::Refuse("an empty engine call".into());
    };
    let opts = value_after(args, "--options-json");

    if same(args, &argv::version()) {
        return GateAnswer::Allow;
    }

    // The Convert page's library line reads the app settings every time the
    // drop area draws. The READ only, whole: app-settings with no --set
    // writes nothing, and the capture points SCREEPUB_CONFIG_DIR at a
    // scratch folder besides. The one write allowed is the reader's device,
    // below; any other --set is refused.
    if same(args, &argv::app_settings(None)) {
        return GateAnswer::Allow;
    }

    // The result screen's device poll and its one-off full listing. The
    // capture points SCREEPUB_VOLUME_ROOTS at an empty scratch folder and the
    // reMarkable at a closed local port (run.rs), so these list nothing real.
    if same(args, &argv::devices(false)) || same(args, &argv::devices(true)) {
        return GateAnswer::Allow;
    }

    // A card chosen on the result screen: written into the capture's own
    // scratch config folder, and only the reader's device, by name.
    for kind in ["kindle", "apple", "kobo", "tolino", "remarkable"] {
        if same(args, &argv::reader_device(kind)) {
            return GateAnswer::Allow;
        }
    }

    let second = args.get(1).map(String::as_str);

    // Read-only questions about the demo book: its routes and its Kindle file.
    if let Some(book) = second.filter(|_| inside(second, ctx.library)) {
        if first == "routes"
            && (same(args, &argv::routes(book, false)) || same(args, &argv::routes(book, true)))
        {
            return GateAnswer::Allow;
        }
        if first == "export" && same(args, &argv::kindle_check(book)) {
            return GateAnswer::Allow;
        }
        if first == "settings" && same(args, &argv::settings(book, value_after(args, "--set"))) {
            return GateAnswer::Allow;
        }
    }

    if Path::new(first).is_absolute()
        && resolve(Path::new(first)) == resolve(ctx.demo_pdf)
        && same(
            args,
            &argv::convert(first, args.iter().any(|a| a == argv::FORCE_FLAG), opts),
        )
    {
        return GateAnswer::Allow;
    }

    let out = value_after(args, "-o");
    if inside(Some(first), ctx.library)
        && inside(out, ctx.library)
        && let (Some(out), Some(opts)) = (out, opts)
        && same(args, &argv::reconvert(first, out, opts))
    {
        return GateAnswer::Allow;
    }

    GateAnswer::Refuse(format!(
        "{}: not a call the window makes on the way to a picture",
        args.join(" ")
    ))
}
