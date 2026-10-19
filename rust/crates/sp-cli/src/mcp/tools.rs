//! The tools: each one a name, a description, an input schema, whether it
//! writes, and a function from checked arguments to a result.
//!
//! The frame ([`ToolOut`], [`ToolError`], [`Tool`], [`Args`]) is Method
//! Budget's `src/mcp/tools/mod.rs`, unchanged but for what only budgets
//! need. The tools are Reader's, and none of them decides anything: each
//! one builds the argv the command line would be given, runs it in this
//! process with `--json` ([`crate::run`], the window's own entry), and
//! reads the answer. Where the command line can only write a file a tool
//! didn't ask for (every conversion writes an EPUB), that file goes to a
//! scratch folder that is removed afterwards. Inspecting and previewing
//! read the classified elements the command line writes with `--debug`.
//!
//! Results are a short summary for people and a JSON object for programs
//! (`structuredContent`). Text from a script (titles, headings, names,
//! dialogue) is returned as data, never as instructions.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use sp_engine::library_images::images_dir_for;
use sp_engine::nodepath::host;

use super::paths::{PDF_EXTS, Roots, SCRIPT_EXTS};
use super::{limits, schema};
use crate::out::Sink;

/// What a tool returns when it works.
pub struct ToolOut {
    /// One or two sentences for a person.
    pub summary: String,
    /// The result for a program.
    pub data: Value,
}

/// Why a tool didn't do what it was asked. Returned to the client as a tool
/// result with `isError: true`, so a model can read it and try again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError {
    /// `invalid_input`, `not_found`, `refused`, `path_refused` or `io`.
    pub kind: &'static str,
    pub message: String,
}

impl ToolError {
    pub fn invalid(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "invalid_input",
            message: m.into(),
        }
    }
    pub fn not_found(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "not_found",
            message: m.into(),
        }
    }
    pub fn refused(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "refused",
            message: m.into(),
        }
    }
    pub fn path(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "path_refused",
            message: m.into(),
        }
    }
    pub fn io(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "io",
            message: m.into(),
        }
    }
}

impl From<String> for ToolError {
    fn from(m: String) -> ToolError {
        ToolError::invalid(m)
    }
}

pub type ToolResult = Result<ToolOut, ToolError>;

/// What every tool works with.
pub struct Ctx {
    pub roots: Roots,
}

/// A tool.
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Writes a file. Hidden (and refused) when the server is read-only.
    pub mutating: bool,
    /// Removes or replaces something (a hint for clients).
    pub destructive: bool,
    pub schema: fn() -> Value,
    pub run: fn(&mut Ctx, &Args) -> ToolResult,
}

impl Tool {
    /// The tool as `tools/list` describes it.
    pub fn describe(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": (self.schema)(),
            "annotations": {
                "title": self.title,
                "readOnlyHint": !self.mutating,
                "destructiveHint": self.mutating && self.destructive,
                "idempotentHint": !self.mutating,
                "openWorldHint": false,
            },
        })
    }
}

/// A tool call's arguments, already checked against the tool's schema.
pub struct Args<'a>(pub &'a Map<String, Value>);

impl Args<'_> {
    pub fn get(&self, k: &str) -> Option<&Value> {
        self.0.get(k).filter(|v| !v.is_null())
    }
    pub fn str(&self, k: &str) -> Option<&str> {
        self.get(k).and_then(Value::as_str)
    }
    pub fn req(&self, k: &str) -> Result<&str, ToolError> {
        self.str(k)
            .ok_or_else(|| ToolError::invalid(format!("`{k}` is required")))
    }
    pub fn bool(&self, k: &str) -> bool {
        self.get(k).and_then(Value::as_bool).unwrap_or(false)
    }
    pub fn u64(&self, k: &str) -> Option<u64> {
        self.get(k).and_then(Value::as_u64)
    }
    pub fn strings(&self, k: &str) -> Vec<String> {
        self.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Script text quoted for a summary line, cut short: it is data, so it is
/// shown, never followed.
pub fn quoted(s: &str) -> String {
    let one_line: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let cut: String = one_line.chars().take(60).collect();
    if cut.chars().count() < one_line.chars().count() {
        format!("\"{cut}...\"")
    } else {
        format!("\"{cut}\"")
    }
}

// ---------------------------------------------------------------------------
// The tools

/// Every tool, in the order `tools/list` gives them.
pub fn all() -> Vec<Tool> {
    vec![
        Tool {
            name: "inspect_script",
            title: "Inspect a script",
            description: "Read a screenplay PDF and say what is in it: its title and author as \
                detected, its page count, every scene heading (with the scene number a shooting \
                script prints, and the page it starts on), and every speaking character with \
                how many speeches they have and the page they first speak on, most speeches \
                first. Writes nothing. A scanned PDF (no text layer) is refused, and so is a \
                document that doesn't read as a screenplay unless force is true.",
            mutating: false,
            destructive: false,
            schema: || schema::object(vec![pdf_prop(), force_prop()], &["path"]),
            run: inspect_script,
        },
        Tool {
            name: "preview_page",
            title: "Preview a page as text",
            description: "One page of a screenplay PDF as plain text, the way Method Reader \
                reads it: each scene heading, action paragraph, character cue, \
                parenthetical, line of dialogue and transition in order, with no emphasis \
                marks. Pages are the PDF's own sheets, counted from 1 (the title page is \
                usually page 1), which may differ from the numbers printed on them. Also \
                returns each element with its kind. Writes nothing.",
            mutating: false,
            destructive: false,
            schema: || {
                schema::object(
                    vec![
                        pdf_prop(),
                        (
                            "page",
                            schema::integer(
                                "The page, counted from 1 as the PDF's sheets are.",
                                1,
                                limits::MAX_PAGE,
                            ),
                        ),
                        force_prop(),
                    ],
                    &["path", "page"],
                )
            },
            run: preview_page,
        },
        Tool {
            name: "convert_to_script",
            title: "Convert to an editable script",
            description: "Turn a screenplay PDF into a script to keep editing: a .fountain \
                (plain text) and a .fdx (beta: a reconstruction, so revision marks and exact \
                page breaks do not carry over). Files are named after the PDF \
                (<name>.fountain, <name>.fdx) and written into output_dir, by default the \
                PDF's own folder. Pictures carried out of the PDF (beta) go in a \
                <name>.images folder beside the .fountain. An existing file is never \
                replaced unless overwrite is true.",
            mutating: true,
            destructive: false,
            schema: || {
                schema::object(
                    vec![
                        pdf_prop(),
                        (
                            "formats",
                            formats_schema(
                                "Which scripts to write (default both).",
                                &["fountain", "fdx"],
                            ),
                        ),
                        output_dir_prop(),
                        overwrite_prop(),
                        force_prop(),
                    ],
                    &["path"],
                )
            },
            run: convert_to_script,
        },
        Tool {
            name: "make_ebook",
            title: "Make an e-book",
            description: "Make an e-book from a screenplay PDF, or from a .fountain script: an \
                EPUB (for most readers and apps), an AZW3 (beta: for Kindles over USB, made by \
                Method Reader from the EPUB) and a MOBI (for the oldest Kindles over USB; text \
                only, no pictures). Files are named after the script (<name>.epub, \
                <name>.azw3, <name>.mobi) and written into output_dir, by default the \
                script's own folder. The book uses the script's saved settings when a \
                <name>.method-reader.json sits beside it, else the app's defaults. An \
                existing file is never replaced unless overwrite is true.",
            mutating: true,
            destructive: false,
            schema: || {
                schema::object(
                    vec![
                        (
                            "path",
                            schema::path(
                                "The script: an absolute path to a .pdf, or to a .fountain \
                                 (or .txt) Fountain script.",
                            ),
                        ),
                        (
                            "formats",
                            formats_schema(
                                "Which e-books to write (default epub).",
                                &["epub", "azw3", "mobi"],
                            ),
                        ),
                        output_dir_prop(),
                        (
                            "title",
                            schema::text("The book's title, instead of the one detected."),
                        ),
                        (
                            "author",
                            schema::text("The book's author, instead of the one detected."),
                        ),
                        overwrite_prop(),
                        force_prop(),
                    ],
                    &["path"],
                )
            },
            run: make_ebook,
        },
    ]
}

fn pdf_prop() -> (&'static str, Value) {
    (
        "path",
        schema::path("The screenplay PDF: an absolute path inside the server's folders."),
    )
}

fn force_prop() -> (&'static str, Value) {
    (
        "force",
        schema::boolean(
            "Read it even if it doesn't look like a screenplay (as method-reader --force does).",
        ),
    )
}

fn output_dir_prop() -> (&'static str, Value) {
    (
        "output_dir",
        schema::path(
            "An absolute path to an existing folder to write into (default: the script's own \
             folder).",
        ),
    )
}

fn overwrite_prop() -> (&'static str, Value) {
    (
        "overwrite",
        schema::boolean("Replace files that are already there (default false: refuse)."),
    )
}

fn formats_schema(description: &str, values: &[&str]) -> Value {
    let mut s = schema::array(
        description,
        schema::one_of("A format.", values),
        values.len(),
    );
    s["minItems"] = json!(1);
    s
}

// ---------------------------------------------------------------------------
// Running the command line

/// The two streams of one in-process run.
#[derive(Clone, Default)]
struct Capture {
    out: Arc<Mutex<Vec<String>>>,
    err: Arc<Mutex<Vec<String>>>,
}

impl Sink for Capture {
    fn out(&mut self, line: &str) {
        self.out
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(line.to_owned());
    }
    fn err(&mut self, line: &str) {
        self.err
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(line.to_owned());
    }
}

/// Runs `method-reader <argv> --json` in this process: its answer, the
/// `{ ok: true, ... }` object, or the error it gave, by kind. What it said
/// on stderr goes to the server's stderr, which is the log.
fn cli(mut argv: Vec<String>) -> Result<Value, ToolError> {
    // The unit tests run in this process, whose environment is this
    // machine's: a conversion there would read the real app settings. They
    // only ever reach refusals; tests/mcp.rs runs the tools in the binary.
    if cfg!(test) {
        panic!("a unit test reached the command line with {argv:?}");
    }
    argv.push("--json".into());
    let capture = Capture::default();
    crate::run(&argv, Box::new(capture.clone()));
    for line in capture.err.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        eprintln!("method-reader mcp: {line}");
    }
    let out = capture.out.lock().unwrap_or_else(|e| e.into_inner());
    let answer: Value = out
        .last()
        .and_then(|l| serde_json::from_str(l).ok())
        .ok_or_else(|| ToolError::io("method-reader gave no answer"))?;
    if answer["ok"] == json!(true) {
        return Ok(answer);
    }
    let code = answer["error"]["code"].as_str().unwrap_or("internal");
    let message = answer["error"]["message"]
        .as_str()
        .unwrap_or("method-reader stopped")
        .to_owned();
    let message = format!("{message} (method-reader error: {code})");
    Err(match code {
        "usage" | "bad-options" => ToolError::invalid(message),
        "unwritable" | "internal" => ToolError::io(message),
        _ => ToolError::refused(message),
    })
}

/// A folder of its own under the system's temporary folder, removed when
/// dropped: where the command line writes what a tool didn't ask for.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch, ToolError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!(
            "method-reader-mcp-{}-{nanos}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir)
            .map_err(|e| ToolError::io(format!("a scratch folder: {e}")))?;
        Ok(Scratch(dir))
    }

    fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The script the `path` argument names, with one of `exts`, as its real
/// path; refused when it is larger than the server reads.
fn input(ctx: &Ctx, a: &Args, exts: &[&str]) -> Result<(PathBuf, String), ToolError> {
    let real = ctx.roots.existing(a.req("path")?, exts)?;
    let size = std::fs::metadata(&real)
        .map_err(|e| ToolError::io(format!("{}: {e}", real.display())))?
        .len();
    if size > limits::MAX_FILE_BYTES {
        return Err(ToolError::refused(format!(
            "{}: larger than the {} MB this server reads",
            real.display(),
            limits::MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    let text = ctx.roots.show(&real);
    Ok((real, text))
}

/// The file name the command line gives what it makes of `input`: its name
/// without the extension.
fn stem_of(input: &str) -> String {
    host::basename_ext(input, &host::extname(input))
}

/// The folder outputs go to: `output_dir`, or the script's own.
fn out_dir(ctx: &Ctx, a: &Args, input: &Path) -> Result<PathBuf, ToolError> {
    match a.str("output_dir") {
        Some(d) => ctx.roots.folder(d),
        None => {
            let parent = input.parent().unwrap_or(Path::new("/"));
            ctx.roots.folder(&parent.to_string_lossy())
        }
    }
}

/// `<dir>/<stem>.<ext>`, checked as a place to write, and refused when
/// something is there already and `overwrite` is false.
fn claim(
    ctx: &Ctx,
    dir: &Path,
    stem: &str,
    ext: &str,
    overwrite: bool,
) -> Result<String, ToolError> {
    let typed = dir.join(format!("{stem}.{ext}"));
    let real = ctx.roots.target(&typed.to_string_lossy(), &[ext])?;
    let text = real.to_string_lossy().into_owned();
    if !overwrite && real.exists() {
        return Err(ToolError::refused(format!(
            "{text} already exists; pass overwrite: true to replace it"
        )));
    }
    Ok(text)
}

/// The flags every conversion takes from the arguments.
fn common_flags(a: &Args, argv: &mut Vec<String>) {
    if a.bool("force") {
        argv.push("--force".into());
    }
    for k in ["title", "author"] {
        if let Some(v) = a.str(k) {
            argv.push(format!("--{k}"));
            argv.push(v.to_owned());
        }
    }
}

/// The classified script, as `--debug` writes it: a conversion into a
/// scratch folder that keeps nothing.
fn classified(a: &Args, input: &str) -> Result<(Value, Value), ToolError> {
    let scratch = Scratch::new()?;
    let stem = stem_of(input);
    let mut argv = vec![
        input.to_owned(),
        "--debug".into(),
        "--no-fountain".into(),
        "-o".into(),
        scratch.file(&format!("{stem}.epub")),
    ];
    common_flags(a, &mut argv);
    let answer = cli(argv)?;
    let path = answer["debugPath"]
        .as_str()
        .ok_or_else(|| ToolError::io("method-reader wrote no classified elements"))?;
    let text = std::fs::read_to_string(path)
        .map_err(|e| ToolError::io(format!("the classified elements: {e}")))?;
    let elements: Value = serde_json::from_str(&text)
        .map_err(|e| ToolError::io(format!("the classified elements: {e}")))?;
    Ok((answer, elements))
}

/// The answer's warnings.
fn warnings(answer: &Value) -> Value {
    answer.get("warnings").cloned().unwrap_or_else(|| json!([]))
}

/// The title and author line of a summary.
fn titled(answer: &Value) -> String {
    let title = quoted(answer["title"].as_str().unwrap_or_default());
    match answer["author"].as_str().filter(|a| !a.is_empty()) {
        Some(a) => format!("{title} by {}", quoted(a)),
        None => title,
    }
}

fn inspect_script(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (_, input) = input(ctx, a, PDF_EXTS)?;
    let (answer, sp) = classified(a, &input)?;
    let elements = sp["elements"].as_array().cloned().unwrap_or_default();
    let page_of = |i: &Value| {
        i.as_u64()
            .and_then(|i| elements.get(usize::try_from(i).ok()?))
            .map_or(Value::Null, |e| e["pageNum"].clone())
    };
    let scenes: Vec<Value> = sp["scenes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|s| {
            let printed = s["elementIndex"]
                .as_u64()
                .and_then(|i| elements.get(usize::try_from(i).ok()?))
                .and_then(|e| e.get("sceneNumber").cloned());
            json!({
                "index": s["sceneNumber"],
                "number": printed.unwrap_or(Value::Null),
                "heading": s["heading"],
                "page": s["pageNum"],
            })
        })
        .collect();
    let characters: Vec<Value> = sp["characters"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|c| {
            json!({
                "name": c["name"],
                "speeches": c["dialogueCount"],
                "first_page": page_of(&c["firstAppearance"]),
            })
        })
        .collect();
    let pages = sp["pageCount"].clone();
    let summary = format!(
        "{}: {} pages, {} scenes, {} speaking characters.",
        titled(&answer),
        pages,
        scenes.len(),
        characters.len()
    );
    Ok(ToolOut {
        summary,
        data: json!({
            "path": input,
            "title": answer["title"],
            "author": answer.get("author").cloned().unwrap_or(Value::Null),
            "pages": pages,
            "scene_count": scenes.len(),
            "scenes": scenes,
            "character_count": characters.len(),
            "characters": characters,
            "warnings": warnings(&answer),
            "settings_path": answer.get("settingsPath").cloned().unwrap_or(Value::Null),
        }),
    })
}

/// Whether an element of this kind continues the speech before it, so it
/// follows on the next line rather than after a blank one (as a .fountain
/// lays a speech out).
fn continues_speech(kind: &str, before: Option<&str>) -> bool {
    matches!(kind, "dialogue" | "parenthetical")
        && matches!(before, Some("character" | "dialogue" | "parenthetical"))
}

fn preview_page(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (_, input) = input(ctx, a, PDF_EXTS)?;
    let page = a
        .u64("page")
        .ok_or_else(|| ToolError::invalid("`page` is required"))?;
    let (_, sp) = classified(a, &input)?;
    let pages = sp["pageCount"].as_u64().unwrap_or(0);
    if page > pages {
        return Err(ToolError::invalid(format!(
            "page {page}: this script has {pages} page{}",
            if pages == 1 { "" } else { "s" }
        )));
    }
    let mut text = String::new();
    let mut shown: Vec<Value> = Vec::new();
    let mut before: Option<String> = None;
    for e in sp["elements"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let kind = e["type"].as_str().unwrap_or_default();
        if e["pageNum"].as_u64() != Some(page)
            || e["isReadable"] == json!(false)
            || kind == "page-number"
        {
            continue;
        }
        let line = if kind == "image" {
            "[picture]".to_owned()
        } else {
            e["text"].as_str().unwrap_or_default().to_owned()
        };
        if !text.is_empty() {
            text.push_str(if continues_speech(kind, before.as_deref()) {
                "\n"
            } else {
                "\n\n"
            });
        }
        text.push_str(&line);
        shown.push(json!({"type": kind, "text": line}));
        before = Some(kind.to_owned());
    }
    let summary = if text.is_empty() {
        format!("Page {page} of {pages} has no text Method Reader keeps.")
    } else {
        format!("Page {page} of {pages}, as plain text:\n\n{text}")
    };
    Ok(ToolOut {
        summary,
        data: json!({
            "path": input,
            "page": page,
            "pages": pages,
            "text": text,
            "elements": shown,
        }),
    })
}

/// The formats asked for, in `known`'s order, or `default` for none.
fn formats(a: &Args, known: &[&'static str], default: &[&'static str]) -> Vec<&'static str> {
    let asked = a.strings("formats");
    if asked.is_empty() {
        return default.to_vec();
    }
    known
        .iter()
        .copied()
        .filter(|k| asked.iter().any(|x| x == k))
        .collect()
}

fn convert_to_script(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (real, input) = input(ctx, a, PDF_EXTS)?;
    let wanted = formats(a, &["fountain", "fdx"], &["fountain", "fdx"]);
    let dir = out_dir(ctx, a, &real)?;
    let stem = stem_of(&input);
    let overwrite = a.bool("overwrite");
    let scratch = Scratch::new()?;
    let mut argv = vec![
        input.clone(),
        "-o".into(),
        scratch.file(&format!("{stem}.epub")),
    ];
    if wanted.contains(&"fountain") {
        let fountain = claim(ctx, &dir, &stem, "fountain", overwrite)?;
        let images = images_dir_for(&fountain);
        if !overwrite && Path::new(&images).exists() {
            return Err(ToolError::refused(format!(
                "{images} already exists; pass overwrite: true to replace the pictures in it"
            )));
        }
        argv.extend(["--fountain".into(), fountain]);
    } else {
        argv.push("--no-fountain".into());
    }
    if wanted.contains(&"fdx") {
        argv.extend(["--fdx".into(), claim(ctx, &dir, &stem, "fdx", overwrite)?]);
    }
    common_flags(a, &mut argv);
    let answer = cli(argv)?;
    let get = |k: &str| answer.get(k).cloned().unwrap_or(Value::Null);
    let written: Vec<&str> = ["fountainPath", "fdxPath"]
        .iter()
        .filter_map(|k| answer[*k].as_str())
        .collect();
    let summary = format!("{}: wrote {}.", titled(&answer), written.join(" and "));
    Ok(ToolOut {
        summary,
        data: json!({
            "path": input,
            "fountain_path": get("fountainPath"),
            "fdx_path": get("fdxPath"),
            "images_path": get("imagesPath"),
            "image_count": get("imageCount"),
            "title": get("title"),
            "author": get("author"),
            "pages": get("pages"),
            "scenes": get("scenes"),
            "characters": get("characters"),
            "warnings": warnings(&answer),
            "settings_path": get("settingsPath"),
        }),
    })
}

fn make_ebook(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (real, input) = input(ctx, a, SCRIPT_EXTS)?;
    let wanted = formats(a, &["epub", "azw3", "mobi"], &["epub"]);
    let dir = out_dir(ctx, a, &real)?;
    let stem = stem_of(&input);
    let overwrite = a.bool("overwrite");
    let scratch = Scratch::new()?;
    // The EPUB is always made; it lands in the scratch folder unless it was
    // asked for. The MOBI lands beside it, so one not asked for alone is
    // copied out of the scratch folder afterwards.
    let epub = if wanted.contains(&"epub") {
        claim(ctx, &dir, &stem, "epub", overwrite)?
    } else {
        scratch.file(&format!("{stem}.epub"))
    };
    let mobi = if wanted.contains(&"mobi") {
        Some(claim(ctx, &dir, &stem, "mobi", overwrite)?)
    } else {
        None
    };
    let mut argv = vec![input.clone(), "-o".into(), epub, "--no-fountain".into()];
    if mobi.is_some() {
        argv.push("--mobi".into());
    }
    if wanted.contains(&"azw3") {
        argv.extend(["--azw3".into(), claim(ctx, &dir, &stem, "azw3", overwrite)?]);
    }
    common_flags(a, &mut argv);
    let answer = cli(argv)?;
    let mut mobi_path = Value::Null;
    if let (Some(target), Some(made)) = (&mobi, answer["mobiPath"].as_str()) {
        if made != target {
            let bytes = std::fs::read(made).map_err(|e| ToolError::io(format!("the MOBI: {e}")))?;
            sp_engine::fsx::write_atomic(target, &bytes)
                .map_err(|e| ToolError::io(format!("could not write {target}: {}", e.message())))?;
        }
        mobi_path = json!(target);
    }
    let epub_path = if wanted.contains(&"epub") {
        answer.get("epubPath").cloned().unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let azw3_path = answer.get("azw3Path").cloned().unwrap_or(Value::Null);
    let written: Vec<&str> = [&epub_path, &azw3_path, &mobi_path]
        .into_iter()
        .filter_map(Value::as_str)
        .collect();
    let summary = format!("{}: wrote {}.", titled(&answer), written.join(", "));
    let get = |k: &str| answer.get(k).cloned().unwrap_or(Value::Null);
    Ok(ToolOut {
        summary,
        data: json!({
            "path": input,
            "epub_path": epub_path,
            "azw3_path": azw3_path,
            "mobi_path": mobi_path,
            "title": get("title"),
            "author": get("author"),
            "pages": get("pages"),
            "warnings": warnings(&answer),
            "settings_path": get("settingsPath"),
        }),
    })
}
