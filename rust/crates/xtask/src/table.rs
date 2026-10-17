//! The commands the parity harness runs, one row each.
//!
//! A row says which inputs it applies to, any setup (files the harness
//! writes into the scratch folder, and earlier runs of the SAME side's CLI,
//! such as a conversion that makes the .epub an `export` row reads), the
//! arguments both CLIs get, extra environment, and which files it leaves
//! behind and how to compare each. Both CLIs always get the same arguments;
//! stdout (the `--json` answer as values, or a person's text with the
//! scratch folder normalized) and the exit code are always compared, and
//! stderr where the row asks for it.
//!
//! Every verb of `method-reader --help` and every flag combination the
//! TypeScript tests exercise has a row, except what would reach outside
//! the scratch folder: `kfx-install` and `route` performers (Books, a
//! browser, Mail), `reveal` that opens a file manager, an `eject` that
//! works, and any Kindle BUILD through Calibre. Their refusals have rows.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// How an output file is compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    /// Byte for byte, but reported as text: the first differing line.
    Text,
    /// A zip, entry by entry: the same names in the same order, the same
    /// compression method, the same uncompressed bytes (a PNG by its
    /// pixels). The deflate streams themselves are not compared.
    ZipEntries,
    /// A folder: the same file names, each byte for byte (a PNG by its
    /// pixels).
    Folder,
    /// A folder tree, recursively: the same paths, each file by its
    /// extension's rule.
    Tree,
    /// JSON compared as values: key order does not count.
    Json,
    /// Text byte for byte, but each inline PNG by its pixels.
    Html,
    /// A PalmDB (MOBI, AZW3) record by record, a PNG record by its pixels.
    PalmDb,
}

/// Which inputs a row applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inputs {
    /// Every PDF, committed and real.
    Pdf,
    /// Every input.
    Any,
    /// Once per run, on the committed screenplay.pdf: refusals, verbs and
    /// help, which do not depend on the script.
    Once,
}

impl Inputs {
    pub fn accepts(self, input: &Path, committed: bool) -> bool {
        let ext = input
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match self {
            Inputs::Any => true,
            Inputs::Pdf => ext.as_deref() == Some("pdf"),
            Inputs::Once => committed && input.file_stem().is_some_and(|s| s == "screenplay"),
        }
    }
}

/// One output a row leaves behind, as a path under the side's scratch
/// folder.
#[derive(Debug, Clone)]
pub struct Output {
    pub file: String,
    pub compare: Compare,
}

/// Where one side's run happens.
pub struct Invocation<'a> {
    /// The input, copied into this side's `in/` folder.
    pub input: &'a Path,
    /// This side's scratch folder: `in/`, `out/`, `config/`, `library/`.
    pub work: &'a Path,
}

impl Invocation<'_> {
    pub fn out(&self, name: &str) -> OsString {
        self.work.join("out").join(name).into_os_string()
    }

    pub fn path(&self, rel: &str) -> OsString {
        self.work.join(rel).into_os_string()
    }

    /// The input's stem, for the files a conversion writes beside it.
    pub fn stem(&self) -> String {
        self.input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// An environment value for one side.
#[derive(Debug, Clone)]
pub enum EnvValue {
    /// A path under the side's scratch folder.
    Work(&'static str),
    Text(&'static str),
    /// A scratch COPY of this machine's Calibre settings folder, made per
    /// side: the only way a row may let Calibre run.
    CalibreCopy,
}

pub type ArgsFn = Box<dyn Fn(&Invocation) -> Vec<OsString>>;
pub type PrepareFn = Box<dyn Fn(&Invocation)>;

/// One command of the CLI, as both sides run it.
pub struct CommandRow {
    pub name: String,
    pub inputs: Inputs,
    pub prepare: Option<PrepareFn>,
    /// Earlier runs, each side with its own CLI.
    pub steps: Vec<ArgsFn>,
    pub args: ArgsFn,
    pub env: Vec<(&'static str, EnvValue)>,
    pub outputs: Vec<Output>,
    pub stderr: bool,
}

impl CommandRow {
    fn new(name: &str, inputs: Inputs, args: ArgsFn) -> CommandRow {
        CommandRow {
            name: name.to_owned(),
            inputs,
            prepare: None,
            steps: Vec::new(),
            args,
            env: Vec::new(),
            outputs: Vec::new(),
            stderr: true,
        }
    }

    fn outputs(mut self, outputs: &[(&str, Compare)]) -> Self {
        self.outputs.extend(outputs.iter().map(|(f, c)| Output {
            file: (*f).to_owned(),
            compare: *c,
        }));
        self
    }

    fn step(mut self, step: ArgsFn) -> Self {
        self.steps.push(step);
        self
    }

    fn prepare(mut self, f: PrepareFn) -> Self {
        self.prepare = Some(f);
        self
    }

    fn env(mut self, key: &'static str, value: EnvValue) -> Self {
        self.env.push((key, value));
        self
    }

    fn no_stderr(mut self) -> Self {
        self.stderr = false;
        self
    }
}

fn os(s: &str) -> OsString {
    OsString::from(s)
}

fn strs(list: &[&str]) -> Vec<OsString> {
    list.iter().map(|s| os(s)).collect()
}

/// Everything a conversion writes, asked for by every flag that writes it.
fn convert_args(inv: &Invocation, extra: &[&str]) -> Vec<OsString> {
    let mut a = vec![
        inv.input.as_os_str().to_owned(),
        os("-o"),
        inv.out("book.epub"),
        os("--mobi"),
        os("--azw3"),
        inv.out("book.azw3"),
        os("--fdx"),
        inv.out("book.fdx"),
        os("--preview-html"),
        inv.out("book.preview.html"),
        os("--debug"),
        os("--json"),
    ];
    if Inputs::Pdf.accepts(inv.input, false) {
        a.push(os("--fountain"));
        a.push(inv.out("script.fountain"));
    }
    a.extend(extra.iter().map(|s| os(s)));
    a
}

const CONVERT_OUTPUTS: &[(&str, Compare)] = &[
    ("out/book.epub", Compare::ZipEntries),
    ("out/book.mobi", Compare::PalmDb),
    ("out/book.azw3", Compare::PalmDb),
    ("out/book.fdx", Compare::Text),
    ("out/book.preview.html", Compare::Html),
    ("out/book.elements.json", Compare::Json),
    ("out/script.fountain", Compare::Text),
    ("out/script.images", Compare::Folder),
];

/// The step every book-reading row starts from: the book, its .fountain
/// and its pictures in out/.
fn make_book() -> ArgsFn {
    Box::new(|inv| {
        vec![
            inv.input.as_os_str().to_owned(),
            os("-o"),
            inv.out("book.epub"),
            os("--fountain"),
            inv.out("script.fountain"),
            os("--json"),
        ]
    })
}

fn write(inv: &Invocation, rel: &str, text: &str) {
    let p: PathBuf = inv.work.join(rel);
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(p, text).expect("a prepared file");
}

fn mkdir(inv: &Invocation, rel: &str) {
    std::fs::create_dir_all(inv.work.join(rel)).expect("a prepared folder");
}

/// A row whose argv is fixed words, with `@IN` for the input, `@W/x` for a
/// path under the scratch folder.
fn words(name: &str, inputs: Inputs, list: &'static [&'static str]) -> CommandRow {
    CommandRow::new(
        name,
        inputs,
        Box::new(move |inv| {
            list.iter()
                .map(|w| {
                    if *w == "@IN" {
                        inv.input.as_os_str().to_owned()
                    } else if let Some(rel) = w.strip_prefix("@W/") {
                        inv.path(rel)
                    } else {
                        os(w)
                    }
                })
                .collect()
        }),
    )
}

/// A closed local port: the reMarkable probe fails at once.
const NO_TABLET: EnvValue = EnvValue::Text("http://127.0.0.1:9");

fn devices_env(row: CommandRow, roots: &'static str) -> CommandRow {
    row.env("METHOD_READER_VOLUME_ROOTS", EnvValue::Work(roots))
        .env("METHOD_READER_REMARKABLE_ENDPOINT", NO_TABLET)
}

fn fake_readers(inv: &Invocation) {
    mkdir(inv, "volumes/Kindle/documents");
    mkdir(inv, "volumes/Kindle/system");
    mkdir(inv, "volumes/KOBOeReader/.kobo");
    mkdir(inv, "volumes/tolino");
    mkdir(inv, "kobo-only/KOBOeReader/.kobo");
    mkdir(inv, "empty");
}

/// Every verb, and every flag of the verb parser but --json and --help.
const VERB_NAMES: &[&str] = &[
    "devices",
    "send",
    "settings",
    "export",
    "update-decision",
    "update-should-check",
    "kfx-status",
    "kfx-install",
    "app-settings",
    "reveal",
    "routes",
    "route",
    "eject",
];

const VERB_FLAG_VALUES: &[(&str, Option<&str>)] = &[
    ("--device", Some("x")),
    ("--set", Some("{}")),
    ("--for", Some("epub")),
    ("--fountain", Some("x.fountain")),
    ("--options-json", Some("{}")),
    ("--out", Some("/x.epub")),
    ("--offered", Some("1.0.0")),
    ("--current", Some("1.0.0")),
    ("--opted-in", None),
    ("--last-checked", Some("0")),
    ("--quick", None),
    ("--check", None),
];

/// Which flags each verb acts on (src/cli.ts's VERB_FLAGS).
fn owns(verb: &str, flag: &str) -> bool {
    let own: &[&str] = match verb {
        "devices" | "routes" => &["--quick"],
        "send" => &["--device"],
        "settings" | "app-settings" => &["--set"],
        "export" => &["--for", "--fountain", "--options-json", "--out", "--check"],
        "update-decision" => &["--offered", "--current"],
        "update-should-check" => &["--opted-in", "--last-checked"],
        "route" => &["--out", "--fountain", "--options-json"],
        _ => &[],
    };
    own.contains(&flag)
}

/// Every row. Order is the order they run and report in.
#[allow(clippy::too_many_lines)]
pub fn commands() -> Vec<CommandRow> {
    use Compare as C;
    use Inputs::{Any, Once, Pdf};
    let mut rows = vec![
        // ---- conversion, on every input ----
        CommandRow::new("convert", Any, Box::new(|inv| convert_args(inv, &[])))
            .outputs(CONVERT_OUTPUTS)
            .no_stderr(),
        // The phone preset's two knobs, so options reach both sides the way
        // the window sends them.
        CommandRow::new(
            "convert-phone",
            Any,
            Box::new(|inv| {
                convert_args(
                    inv,
                    &[
                        "--options-json",
                        r#"{"dialogueSideMarginPct":10,"dualDialogue":"sequential"}"#,
                    ],
                )
            }),
        )
        .outputs(CONVERT_OUTPUTS)
        .no_stderr(),
        // A person's view: no --json, the companions found by default.
        words(
            "convert-human",
            Any,
            &["@IN", "-o", "@W/out/book.epub", "--mobi"],
        )
        .outputs(&[
            ("out/book.epub", C::ZipEntries),
            ("out/book.mobi", C::PalmDb),
            ("out/book.fountain", C::Text),
            ("out/book.images", C::Folder),
        ]),
        // The progress lines on stderr, and the first page.
        words(
            "convert-progress",
            Any,
            &[
                "@IN",
                "-o",
                "@W/out/book.epub",
                "--no-fountain",
                "--json",
                "--progress",
            ],
        ),
        // The window's own call: the document inline in the answer.
        words(
            "convert-inline",
            Any,
            &[
                "@IN",
                "-o",
                "@W/out/book.epub",
                "--no-fountain",
                "--json",
                "--preview-inline",
            ],
        ),
        // Beside the input, every default name.
        words("convert-beside", Any, &["@IN", "--json", "--mobi"]).outputs(&[("in", C::Tree)]),
        // Into the library; the second time it finds what the first left.
        words("convert-library", Any, &["@IN", "--library", "--json"])
            .outputs(&[("library", C::Tree)]),
        words(
            "convert-library-again",
            Pdf,
            &["@IN", "--library", "--json", "--mobi"],
        )
        .step(Box::new(|inv| {
            vec![
                inv.input.as_os_str().to_owned(),
                os("--library"),
                os("--json"),
            ]
        }))
        .outputs(&[("library", C::Tree)]),
        // A script tuned beside its PDF: the sidecar is used, and adopted.
        // The three sidecar rows write it under its OLD name
        // (`<stem>.screepub.json`, the 0.12.x name), on purpose: the engine
        // writes `<stem>.method-reader.json` now and still reads the old
        // name when the new one is missing, and these rows pin that
        // fallback against the frozen answers. Do not rename them.
        words("convert-sidecar", Pdf, &["@IN", "--library", "--json"])
            .prepare(Box::new(|inv| {
                write(
                    inv,
                    &format!("in/{}.screepub.json", inv.stem()),
                    r#"{"elementSpacingEm":1.5,"fontFamily":"serif"}"#,
                );
            }))
            .outputs(&[("library", C::Tree), ("out", C::Tree)]),
        words(
            "convert-sidecar-override",
            Pdf,
            &[
                "@IN",
                "-o",
                "@W/out/book.epub",
                "--json",
                "--options-json",
                r#"{"fontFamily":"courier"}"#,
            ],
        )
        .prepare(Box::new(|inv| {
            write(
                inv,
                &format!("in/{}.screepub.json", inv.stem()),
                r#"{"elementSpacingEm":1.5,"fontFamily":"serif"}"#,
            );
        }))
        .outputs(&[("out/book.epub", C::ZipEntries)]),
        words(
            "convert-sidecar-broken",
            Pdf,
            &["@IN", "-o", "@W/out/book.epub", "--json"],
        )
        .prepare(Box::new(|inv| {
            write(inv, &format!("in/{}.screepub.json", inv.stem()), "[1, 2]");
        }))
        .outputs(&[("out/book.epub", C::ZipEntries)]),
        // The app's own defaults, and the choice not to pin them.
        words("convert-app-defaults", Pdf, &["@IN", "--library", "--json"])
            .step(Box::new(|_| {
                strs(&[
                    "app-settings",
                    "--json",
                    "--set",
                    r#"{"formatDefaults":{"scenePageBreaks":true,"cueAlignment":"indented"}}"#,
                ])
            }))
            .outputs(&[("library", C::Tree), ("config", C::Tree)]),
        words(
            "convert-follow-defaults",
            Pdf,
            &["@IN", "--library", "--json"],
        )
        .step(Box::new(|_| {
            strs(&[
                "app-settings",
                "--json",
                "--set",
                r#"{"keepScriptSettings":false}"#,
            ])
        }))
        .outputs(&[("library", C::Tree)]),
        words(
            "convert-options-file",
            Pdf,
            &[
                "@IN",
                "-o",
                "@W/out/book.epub",
                "--json",
                "--options",
                "@W/in/opts.json",
            ],
        )
        .prepare(Box::new(|inv| {
            write(
                inv,
                "in/opts.json",
                r#"{"showSceneNumbers":true,"includeTitlePage":false,"justifyText":true}"#,
            );
        }))
        .outputs(&[
            ("out/book.epub", C::ZipEntries),
            ("out/book.fountain", C::Text),
        ]),
        words(
            "convert-title",
            Pdf,
            &[
                "@IN",
                "-o",
                "@W/out/book.epub",
                "--json",
                "--mobi",
                "--title",
                "An Invented Title",
                "--author",
                "Nobody In Particular",
            ],
        )
        .outputs(&[
            ("out/book.epub", C::ZipEntries),
            ("out/book.mobi", C::PalmDb),
            ("out/book.fountain", C::Text),
        ]),
        words(
            "convert-force",
            Pdf,
            &[
                "@IN",
                "-o",
                "@W/out/book.epub",
                "--no-fountain",
                "--json",
                "--force",
            ],
        )
        .outputs(&[("out/book.epub", C::ZipEntries)]),
        words(
            "convert-no-pictures",
            Pdf,
            &[
                "@IN",
                "-o",
                "@W/out/book.epub",
                "--json",
                "--options-json",
                r#"{"includeImages":false}"#,
            ],
        )
        .outputs(&[("out", C::Tree)]),
        // A .fountain input: the books a PDF's .fountain makes again.
        words(
            "convert-fountain",
            Pdf,
            &[
                "@W/out/script.fountain",
                "-o",
                "@W/out/f.epub",
                "--mobi",
                "--azw3",
                "@W/out/f.azw3",
                "--fdx",
                "@W/out/f.fdx",
                "--preview-html",
                "@W/out/f.html",
                "--json",
            ],
        )
        .step(make_book())
        .outputs(&[
            ("out/f.epub", C::ZipEntries),
            ("out/f.mobi", C::PalmDb),
            ("out/f.azw3", C::PalmDb),
            ("out/f.fdx", C::Text),
            ("out/f.html", C::Html),
        ]),
        words(
            "convert-fountain-strip",
            Pdf,
            &[
                "@W/out/script.fountain",
                "-o",
                "@W/out/f.epub",
                "--json",
                "--options-json",
                r#"{"contdMode":"strip","includeImages":false}"#,
            ],
        )
        .step(Box::new(|inv| {
            vec![
                inv.input.as_os_str().to_owned(),
                os("-o"),
                inv.out("book.epub"),
                os("--fountain"),
                inv.out("script.fountain"),
                os("--json"),
                os("--options-json"),
                os(r#"{"contdMode":"keep"}"#),
            ]
        }))
        .outputs(&[("out/f.epub", C::ZipEntries)]),
        words("convert-txt", Pdf, &["@W/out/script.txt", "--json"])
            .step(Box::new(|inv| {
                vec![
                    inv.input.as_os_str().to_owned(),
                    os("-o"),
                    inv.out("book.epub"),
                    os("--fountain"),
                    inv.out("script.txt"),
                    os("--json"),
                ]
            }))
            .outputs(&[("out", C::Tree)]),
        // ---- conversion refusals, once ----
        words("refuse-no-input-json", Once, &["--json"]),
        words("refuse-no-input", Once, &[]),
        words("refuse-two-inputs", Once, &["@IN", "@IN", "--json"]),
        words("refuse-preview-inline", Once, &["@IN", "--preview-inline"]),
        words(
            "refuse-fountain-twice",
            Once,
            &[
                "@IN",
                "--fountain",
                "@W/out/x.fountain",
                "--no-fountain",
                "--json",
            ],
        ),
        words(
            "refuse-fountain-on-fountain",
            Once,
            &[
                "@W/in/x.fountain",
                "--fountain",
                "@W/out/x.fountain",
                "--json",
            ],
        ),
        words(
            "refuse-library-and-o",
            Once,
            &["@IN", "--library", "-o", "@W/out/x.epub", "--json"],
        ),
        words(
            "refuse-o-no-folder",
            Once,
            &["@IN", "-o", "@W/nope/x.epub", "--json"],
        ),
        words(
            "refuse-o-no-folder-human",
            Once,
            &["@IN", "-o", "@W/nope/x.epub"],
        ),
        words(
            "refuse-o-is-folder",
            Once,
            &["@IN", "-o", "@W/out", "--json"],
        ),
        words(
            "refuse-o-under-file",
            Once,
            &["@IN", "-o", "@W/in/f.txt/x.epub", "--json"],
        )
        .prepare(Box::new(|inv| write(inv, "in/f.txt", "x"))),
        words(
            "refuse-fountain-no-folder",
            Once,
            &["@IN", "--fountain", "@W/nope/x.fountain", "--json"],
        ),
        words(
            "refuse-preview-no-folder",
            Once,
            &["@IN", "--preview-html", "@W/nope/x.html", "--json"],
        ),
        words(
            "refuse-fdx-no-folder",
            Once,
            &["@IN", "--fdx", "@W/nope/x.fdx", "--json"],
        ),
        words(
            "refuse-azw3-no-folder",
            Once,
            &["@IN", "--azw3", "@W/nope/x.azw3", "--json"],
        ),
        words(
            "refuse-options-both",
            Once,
            &[
                "@IN",
                "--options",
                "@W/in/o.json",
                "--options-json",
                "{}",
                "--json",
            ],
        ),
        words(
            "refuse-options-json-bad",
            Once,
            &["@IN", "--options-json", "{nope", "--json"],
        ),
        words(
            "refuse-options-json-array",
            Once,
            &["@IN", "--options-json", "[1]", "--json"],
        ),
        words(
            "refuse-options-file-missing",
            Once,
            &["@IN", "--options", "@W/in/none.json", "--json"],
        ),
        words(
            "refuse-options-file-bad",
            Once,
            &["@IN", "--options", "@W/in/o.json", "--json"],
        )
        .prepare(Box::new(|inv| write(inv, "in/o.json", "{not json"))),
        words("refuse-unsupported", Once, &["@W/in/notes.docx", "--json"]),
        words("refuse-unsupported-human", Once, &["@W/in/notes.docx"]),
        words("refuse-verb-after-flag", Once, &["--json", "devices"]),
        words(
            "refuse-unknown-flag",
            Once,
            &["@IN", "--frobnicate", "--json"],
        ),
        words("refuse-unknown-flag-human", Once, &["@IN", "--frobnicate"]),
        words("refuse-missing-value", Once, &["@IN", "--json", "--title"]),
        words("refuse-ambiguous-value", Once, &["@IN", "-o", "--json"]),
        words("refuse-missing-input", Once, &["@W/in/none.pdf", "--json"]),
        words(
            "refuse-missing-fountain",
            Once,
            &["@W/in/none.fountain", "--json"],
        ),
        words("refuse-not-a-pdf", Once, &["@W/in/fake.pdf", "--json"]).prepare(Box::new(|inv| {
            write(inv, "in/fake.pdf", "this is not a PDF at all\n")
        })),
        words("refuse-folder-input", Once, &["@W/in/dir.pdf", "--json"])
            .prepare(Box::new(|inv| mkdir(inv, "in/dir.pdf"))),
        words("version", Once, &["--version"]),
        words("version-json", Once, &["--version", "--json"]),
        words("help", Once, &["--help"]),
        words("help-short", Once, &["-h"]),
        words("help-json", Once, &["--help", "--json"]),
        // A file named like a verb is a file.
        words("verb-shadowed-by-file", Once, &["devices", "--json"])
            .prepare(Box::new(|inv| write(inv, "in/devices", "not a script"))),
        // ---- devices and send ----
        devices_env(words("devices", Once, &["devices", "--json"]), "volumes")
            .prepare(Box::new(fake_readers)),
        devices_env(words("devices-human", Once, &["devices"]), "volumes")
            .prepare(Box::new(fake_readers)),
        devices_env(
            words("devices-quick", Once, &["devices", "--quick", "--json"]),
            "volumes",
        )
        .prepare(Box::new(fake_readers)),
        devices_env(words("devices-none", Once, &["devices", "--json"]), "empty")
            .prepare(Box::new(fake_readers)),
        devices_env(words("devices-none-human", Once, &["devices"]), "empty")
            .prepare(Box::new(fake_readers)),
        devices_env(
            words("devices-argument", Once, &["devices", "extra", "--json"]),
            "empty",
        ),
        devices_env(
            words("send-one", Pdf, &["send", "@W/out/book.epub", "--json"]),
            "kobo-only",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book())
        .outputs(&[("kobo-only", C::Tree), ("config", C::Tree)]),
        devices_env(
            words("send-human", Once, &["send", "@W/out/book.epub"]),
            "kobo-only",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book())
        .outputs(&[("kobo-only", C::Tree)]),
        devices_env(
            words(
                "send-device",
                Once,
                &[
                    "send",
                    "@W/out/book.epub",
                    "--device",
                    "@W/volumes/tolino",
                    "--json",
                ],
            ),
            "volumes",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book())
        .outputs(&[("volumes", C::Tree), ("config", C::Tree)]),
        devices_env(
            words(
                "send-ambiguous",
                Once,
                &["send", "@W/out/book.epub", "--json"],
            ),
            "volumes",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book()),
        devices_env(
            words(
                "send-unknown-device",
                Once,
                &["send", "@W/out/book.epub", "--device", "/nowhere", "--json"],
            ),
            "volumes",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book()),
        devices_env(
            words(
                "send-no-devices",
                Once,
                &["send", "@W/out/book.epub", "--json"],
            ),
            "empty",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book()),
        devices_env(
            words(
                "send-missing-file",
                Once,
                &["send", "@W/out/none.epub", "--json"],
            ),
            "volumes",
        )
        .prepare(Box::new(fake_readers)),
        devices_env(words("send-no-file", Once, &["send", "--json"]), "volumes"),
        devices_env(
            words("eject-relative", Once, &["eject", "Kindle", "--json"]),
            "volumes",
        ),
        devices_env(
            words("eject-not-a-reader", Once, &["eject", "@W/in", "--json"]),
            "volumes",
        )
        .prepare(Box::new(fake_readers)),
        devices_env(
            words("eject-nothing", Once, &["eject", "--json"]),
            "volumes",
        ),
        devices_env(
            words("eject-human-refusal", Once, &["eject", "relative"]),
            "volumes",
        ),
        words("reveal-relative", Once, &["reveal", "book.epub", "--json"]),
        words(
            "reveal-missing",
            Once,
            &["reveal", "@W/in/none.epub", "--json"],
        ),
        words("reveal-folder", Once, &["reveal", "@W/in", "--json"]),
        words("reveal-nothing", Once, &["reveal", "--json"]),
        // ---- settings and app-settings ----
        words(
            "settings",
            Pdf,
            &["settings", "@W/out/script.fountain", "--json"],
        )
        .step(make_book()),
        words(
            "settings-set",
            Once,
            &[
                "settings",
                "@W/out/script.fountain",
                "--json",
                "--set",
                r#"{"elementSpacingEm":1.25,"contdMode":"strip","showSceneNumbers":true}"#,
            ],
        )
        .step(make_book())
        .outputs(&[("out", C::Tree)]),
        words(
            "settings-human",
            Once,
            &["settings", "@W/out/script.fountain"],
        )
        .step(make_book()),
        words(
            "settings-set-bad",
            Once,
            &[
                "settings",
                "@W/out/script.fountain",
                "--set",
                "{nope",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "settings-set-array",
            Once,
            &[
                "settings",
                "@W/out/script.fountain",
                "--set",
                "[]",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "settings-missing",
            Once,
            &["settings", "@W/out/none.fountain", "--json"],
        ),
        words("settings-nothing", Once, &["settings", "--json"]),
        words("app-settings", Once, &["app-settings", "--json"]).outputs(&[("config", C::Tree)]),
        words("app-settings-human", Once, &["app-settings"]),
        words(
            "app-settings-library",
            Once,
            &[
                "app-settings",
                "--json",
                "--set",
                r#"{"keepScriptSettings":false,"readerDevice":"kobo","animatePage":false}"#,
            ],
        )
        .outputs(&[("config", C::Tree)]),
        words(
            "app-settings-defaults",
            Once,
            &[
                "app-settings",
                "--json",
                "--set",
                r#"{"formatDefaults":{"fontFamily":"serif"}}"#,
            ],
        )
        .outputs(&[("config", C::Tree)]),
        words(
            "app-settings-reset",
            Once,
            &[
                "app-settings",
                "--json",
                "--set",
                r#"{"formatDefaults":null,"keepScriptSettings":null,"libraryPath":null}"#,
            ],
        )
        .step(Box::new(|_| {
            strs(&[
                "app-settings",
                "--json",
                "--set",
                r#"{"formatDefaults":{"fontFamily":"serif"},"keepScriptSettings":false}"#,
            ])
        }))
        .outputs(&[("config", C::Tree)]),
        words(
            "app-settings-bad",
            Once,
            &["app-settings", "--json", "--set", "{nope"],
        ),
        words(
            "app-settings-unknown-key",
            Once,
            &["app-settings", "--json", "--set", r#"{"colour":"red"}"#],
        ),
        words(
            "app-settings-relative",
            Once,
            &[
                "app-settings",
                "--json",
                "--set",
                r#"{"libraryPath":"relative/folder"}"#,
            ],
        ),
        words(
            "app-settings-argument",
            Once,
            &["app-settings", "extra", "--json"],
        ),
        // ---- export ----
        words(
            "export-epub",
            Pdf,
            &["export", "@W/out/book.epub", "--json"],
        )
        .step(make_book()),
        words(
            "export-epub-out",
            Once,
            &[
                "export",
                "@W/out/book.epub",
                "--json",
                "--out",
                "@W/out/copy.epub",
            ],
        )
        .step(make_book())
        .outputs(&[("out/copy.epub", C::ZipEntries)]),
        words(
            "export-azw3",
            Pdf,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "azw3",
                "--json",
                "--out",
                "@W/out/copy.azw3",
            ],
        )
        .step(make_book())
        .outputs(&[("out/book.azw3", C::PalmDb), ("out/copy.azw3", C::PalmDb)]),
        words(
            "export-mobi",
            Pdf,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "mobi",
                "--fountain",
                "@W/out/script.fountain",
                "--json",
            ],
        )
        .step(make_book())
        .outputs(&[("out/book.mobi", C::PalmDb)]),
        words(
            "export-fountain",
            Pdf,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "fountain",
                "--fountain",
                "@W/out/script.fountain",
                "--out",
                "@W/out/copy.fountain",
                "--json",
            ],
        )
        .step(make_book())
        .outputs(&[("out/copy.fountain", C::Text)]),
        words(
            "export-fdx",
            Pdf,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "fdx",
                "--fountain",
                "@W/out/script.fountain",
                "--out",
                "@W/out/copy.fdx",
                "--json",
            ],
        )
        .step(make_book())
        .outputs(&[("out/copy.fdx", C::Text)]),
        words(
            "export-human",
            Once,
            &["export", "@W/out/book.epub", "--for", "azw3"],
        )
        .step(make_book()),
        words(
            "export-check-azw3",
            Once,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "azw3",
                "--check",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "export-check-mobi",
            Once,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "mobi",
                "--check",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "export-check-kindle",
            Once,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "kindle",
                "--check",
                "--json",
            ],
        )
        .step(make_book())
        .env("CALIBRE_CONFIG_DIRECTORY", EnvValue::CalibreCopy),
        words(
            "export-check-kfx",
            Once,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "kfx",
                "--check",
                "--json",
            ],
        )
        .step(make_book())
        .env("CALIBRE_CONFIG_DIRECTORY", EnvValue::CalibreCopy),
        words(
            "export-bad-for",
            Once,
            &["export", "@W/out/book.epub", "--for", "docx", "--json"],
        )
        .step(make_book()),
        words(
            "export-mobi-no-fountain",
            Once,
            &["export", "@W/out/book.epub", "--for", "mobi", "--json"],
        )
        .step(make_book()),
        words(
            "export-out-relative",
            Once,
            &["export", "@W/out/book.epub", "--out", "copy.epub", "--json"],
        )
        .step(make_book()),
        words(
            "export-out-wrong-extension",
            Once,
            &[
                "export",
                "@W/out/book.epub",
                "--out",
                "@W/out/copy.mobi",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "export-bad-options",
            Once,
            &[
                "export",
                "@W/out/book.epub",
                "--for",
                "mobi",
                "--fountain",
                "@W/out/script.fountain",
                "--options-json",
                "[]",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "export-missing",
            Once,
            &["export", "@W/out/none.epub", "--json"],
        ),
        words("export-nothing", Once, &["export", "--json"]),
        // ---- routes and route ----
        devices_env(
            words(
                "routes-quick",
                Once,
                &["routes", "@W/out/book.epub", "--quick", "--json"],
            ),
            "kobo-only",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book()),
        devices_env(
            words("routes", Once, &["routes", "@W/out/book.epub", "--json"]),
            "kobo-only",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book()),
        devices_env(
            words(
                "routes-human",
                Once,
                &["routes", "@W/out/book.epub", "--quick"],
            ),
            "kobo-only",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book()),
        devices_env(
            words(
                "routes-after-send",
                Once,
                &["routes", "@W/out/book.epub", "--quick", "--json"],
            ),
            "kobo-only",
        )
        .prepare(Box::new(fake_readers))
        .step(make_book())
        .step(Box::new(|inv| {
            vec![os("send"), inv.out("book.epub"), os("--json")]
        })),
        words(
            "routes-missing",
            Once,
            &["routes", "@W/out/none.epub", "--json"],
        ),
        words("routes-nothing", Once, &["routes", "--json"]),
        words(
            "route-save-epub",
            Once,
            &[
                "route",
                "save-epub",
                "@W/out/book.epub",
                "--out",
                "@W/out/saved.epub",
                "--json",
            ],
        )
        .step(make_book())
        .outputs(&[("out/saved.epub", C::ZipEntries), ("config", C::Tree)]),
        words(
            "route-save-epub-human",
            Once,
            &[
                "route",
                "save-epub",
                "@W/out/book.epub",
                "--out",
                "@W/out/saved.epub",
            ],
        )
        .step(make_book()),
        words(
            "route-save-no-out",
            Once,
            &["route", "save-epub", "@W/out/book.epub", "--json"],
        )
        .step(make_book()),
        words(
            "route-save-wrong-extension",
            Once,
            &[
                "route",
                "save-epub",
                "@W/out/book.epub",
                "--out",
                "@W/out/saved.azw3",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "route-unknown-key",
            Once,
            &["route", "carrier-pigeon", "@W/out/book.epub", "--json"],
        )
        .step(make_book()),
        words(
            "route-out-on-performer",
            Once,
            &[
                "route",
                "apple-books",
                "@W/out/book.epub",
                "--out",
                "@W/out/x.epub",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "route-fountain-on-save-epub",
            Once,
            &[
                "route",
                "save-epub",
                "@W/out/book.epub",
                "--out",
                "@W/out/x.epub",
                "--fountain",
                "@W/out/script.fountain",
                "--json",
            ],
        )
        .step(make_book()),
        words(
            "route-setup-with-book",
            Once,
            &["route", "kindle-email-setup", "@W/out/book.epub", "--json"],
        )
        .step(make_book()),
        words(
            "route-missing-book",
            Once,
            &[
                "route",
                "save-epub",
                "@W/out/none.epub",
                "--out",
                "@W/out/x.epub",
                "--json",
            ],
        ),
        words("route-nothing", Once, &["route", "--json"]),
        words(
            "route-too-many",
            Once,
            &["route", "save-epub", "a.epub", "b.epub", "--json"],
        ),
        // ---- KFX ----
        words("kfx-status", Once, &["kfx-status", "--json"])
            .env("CALIBRE_CONFIG_DIRECTORY", EnvValue::CalibreCopy),
        words("kfx-status-human", Once, &["kfx-status"])
            .env("CALIBRE_CONFIG_DIRECTORY", EnvValue::CalibreCopy),
        words(
            "kfx-status-argument",
            Once,
            &["kfx-status", "extra", "--json"],
        ),
        words(
            "kfx-install-argument",
            Once,
            &["kfx-install", "extra", "--json"],
        ),
        // ---- updates (offline) ----
        words(
            "update-offer",
            Once,
            &[
                "update-decision",
                "--offered",
                "0.12.0",
                "--current",
                "0.11.1",
                "--json",
            ],
        ),
        words(
            "update-offer-human",
            Once,
            &[
                "update-decision",
                "--offered",
                "v0.12.0",
                "--current",
                "0.11.1",
            ],
        ),
        words(
            "update-same",
            Once,
            &[
                "update-decision",
                "--offered",
                "0.11.1",
                "--current",
                "0.11.1",
                "--json",
            ],
        ),
        words(
            "update-older",
            Once,
            &[
                "update-decision",
                "--offered",
                "0.10.0",
                "--current",
                "0.11.1",
                "--json",
            ],
        ),
        words(
            "update-describe",
            Once,
            &[
                "update-decision",
                "--offered",
                "0.11.1",
                "--current",
                "0.11.1-3-gabc1234",
                "--json",
            ],
        ),
        words(
            "update-prerelease",
            Once,
            &[
                "update-decision",
                "--offered",
                "0.12.0-beta.2",
                "--current",
                "0.12.0-beta.10",
                "--json",
            ],
        ),
        words(
            "update-garbled",
            Once,
            &[
                "update-decision",
                "--offered",
                "latest",
                "--current",
                "0.11.1",
                "--json",
            ],
        ),
        words(
            "update-refuse-human",
            Once,
            &[
                "update-decision",
                "--offered",
                "0.10.0",
                "--current",
                "0.11.1",
            ],
        ),
        words(
            "update-no-offered",
            Once,
            &["update-decision", "--current", "0.11.1", "--json"],
        ),
        words(
            "update-no-current",
            Once,
            &["update-decision", "--offered", "0.11.1", "--json"],
        ),
        words("update-argument", Once, &["update-decision", "x", "--json"]),
        words("should-check-off", Once, &["update-should-check", "--json"]),
        words(
            "should-check-never",
            Once,
            &["update-should-check", "--opted-in", "--json"],
        ),
        words(
            "should-check-long-ago",
            Once,
            &[
                "update-should-check",
                "--opted-in",
                "--last-checked",
                "1000",
                "--json",
            ],
        ),
        words(
            "should-check-future",
            Once,
            &[
                "update-should-check",
                "--opted-in",
                "--last-checked",
                "99999999999999",
                "--json",
            ],
        ),
        words(
            "should-check-bad",
            Once,
            &[
                "update-should-check",
                "--opted-in",
                "--last-checked",
                "yesterday",
                "--json",
            ],
        ),
        words(
            "should-check-human",
            Once,
            &["update-should-check", "--opted-in"],
        ),
        words(
            "should-check-argument",
            Once,
            &["update-should-check", "x", "--json"],
        ),
    ];
    // Every verb's help, both ways, and an unknown flag in its JSON mode.
    for verb in VERB_NAMES {
        rows.push(CommandRow::new(
            &format!("{verb}-help"),
            Once,
            Box::new(move |_| strs(&[verb, "--help"])),
        ));
        rows.push(CommandRow::new(
            &format!("{verb}-help-json"),
            Once,
            Box::new(move |_| strs(&[verb, "-h", "--json"])),
        ));
        rows.push(CommandRow::new(
            &format!("{verb}-unknown-flag"),
            Once,
            Box::new(move |_| strs(&[verb, "--json", "--no-such-flag"])),
        ));
    }
    // Every verb against every flag it does not act on (cli-verb-flags):
    // each refusal, before any handler runs. Positionals are given so a
    // verb that needs one cannot refuse for that first.
    for verb in VERB_NAMES {
        for (flag, value) in VERB_FLAG_VALUES {
            if owns(verb, flag) {
                continue;
            }
            rows.push(
                CommandRow::new(
                    &format!("{verb}-refuses{flag}"),
                    Once,
                    Box::new(move |_| {
                        let mut a = strs(&[verb, "--json", flag]);
                        if let Some(v) = value {
                            a.push(os(v));
                        }
                        a
                    }),
                )
                .env("METHOD_READER_VOLUME_ROOTS", EnvValue::Work("empty"))
                .env("METHOD_READER_REMARKABLE_ENDPOINT", NO_TABLET),
            );
        }
    }
    // Several foreign flags at once: the order they are refused in.
    rows.push(words(
        "verb-flag-order",
        Once,
        &[
            "settings",
            "--json",
            "--device",
            "x",
            "--offered",
            "1",
            "--quick",
        ],
    ));
    rows.push(words(
        "verb-flag-order-update",
        Once,
        &[
            "update-decision",
            "--json",
            "--quick",
            "--offered",
            "1",
            "--set",
            "{}",
        ],
    ));
    rows
}
