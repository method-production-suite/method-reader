//! The commands the parity harness runs, one row each.
//!
//! A row says which inputs it applies to, the arguments both CLIs get, and
//! which files it leaves behind and how to compare each. Both CLIs always
//! get the same arguments; `--json`'s answer on stdout and the exit code are
//! always compared, so a row lists only its files.
//!
//! Adding a command (devices, settings, export, ...) is adding a row. A
//! command that needs a file prepared first (a .fountain to `settings`, an
//! .epub to `export`) can point `args` at what an earlier step in its own
//! `args` builder wrote into `Invocation::work`.

use std::ffi::OsString;
use std::path::Path;

/// How an output file is compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    /// Byte for byte (MOBI, AZW3, FDX, PNG).
    Bytes,
    /// Byte for byte, but reported as text: the first differing line.
    Text,
    /// A zip, entry by entry: the same names in the same order, the same
    /// compression method, the same uncompressed bytes. The deflate streams
    /// themselves are not compared, because two correct deflaters differ.
    ZipEntries,
    /// A folder: the same file names, each byte for byte.
    Folder,
}

/// Which inputs a row applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inputs {
    Pdf,
    /// No row is .fountain-only yet; the real scripts include .fountain
    /// inputs, which the `Any` rows already take.
    #[allow(dead_code)]
    Fountain,
    Any,
}

impl Inputs {
    pub fn accepts(self, input: &Path) -> bool {
        let ext = input
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match self {
            Inputs::Any => true,
            Inputs::Pdf => ext.as_deref() == Some("pdf"),
            Inputs::Fountain => ext.as_deref() == Some("fountain"),
        }
    }
}

/// One output a row leaves in its `out/` folder.
#[derive(Debug, Clone, Copy)]
pub struct Output {
    /// Path under `out/`.
    pub file: &'static str,
    pub compare: Compare,
}

/// Where one side's run happens.
pub struct Invocation<'a> {
    /// The input, copied into this side's own folder.
    pub input: &'a Path,
    /// This side's output folder (empty at the start).
    pub out: &'a Path,
}

/// One command of the CLI, as both sides run it.
pub struct CommandRow {
    pub name: &'static str,
    pub inputs: Inputs,
    pub args: fn(&Invocation) -> Vec<OsString>,
    pub outputs: &'static [Output],
}

fn os(s: &str) -> OsString {
    OsString::from(s)
}

/// Everything a conversion writes, asked for by every flag that writes it.
fn convert_args(inv: &Invocation, extra: &[&str]) -> Vec<OsString> {
    let mut a = vec![
        inv.input.as_os_str().to_owned(),
        os("-o"),
        inv.out.join("book.epub").into_os_string(),
        os("--mobi"),
        os("--azw3"),
        inv.out.join("book.azw3").into_os_string(),
        os("--fdx"),
        inv.out.join("book.fdx").into_os_string(),
        os("--preview-html"),
        inv.out.join("book.preview.html").into_os_string(),
        os("--debug"),
        os("--json"),
    ];
    if Inputs::Pdf.accepts(inv.input) {
        a.push(os("--fountain"));
        a.push(inv.out.join("script.fountain").into_os_string());
    }
    a.extend(extra.iter().map(|s| os(s)));
    a
}

const CONVERT_OUTPUTS: &[Output] = &[
    Output {
        file: "book.epub",
        compare: Compare::ZipEntries,
    },
    Output {
        file: "book.mobi",
        compare: Compare::Bytes,
    },
    Output {
        file: "book.azw3",
        compare: Compare::Bytes,
    },
    Output {
        file: "book.fdx",
        compare: Compare::Text,
    },
    Output {
        file: "book.preview.html",
        compare: Compare::Text,
    },
    Output {
        file: "book.elements.json",
        compare: Compare::Text,
    },
    Output {
        file: "script.fountain",
        compare: Compare::Text,
    },
    Output {
        file: "script.images",
        compare: Compare::Folder,
    },
];

/// Every row. Order is the order they run and report in.
pub const COMMANDS: &[CommandRow] = &[
    CommandRow {
        name: "convert",
        inputs: Inputs::Any,
        args: |inv| convert_args(inv, &[]),
        outputs: CONVERT_OUTPUTS,
    },
    // The phone preset's two knobs, so options reach both sides the way the
    // window sends them.
    CommandRow {
        name: "convert-phone",
        inputs: Inputs::Any,
        args: |inv| {
            convert_args(
                inv,
                &[
                    "--options-json",
                    r#"{"dialogueSideMarginPct":10,"dualDialogue":"sequential"}"#,
                ],
            )
        },
        outputs: CONVERT_OUTPUTS,
    },
];
