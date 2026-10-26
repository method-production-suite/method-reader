//! The conversion path of src/cli.ts: the flags, the refusals in their
//! order, the settings layers, the run itself, every output write, the
//! library pin, and the answer.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use sp_engine::convert::{ConvertError, ConvertOptions, ConvertResult, ConvertStage};
use sp_engine::env::{Env, host_platform};
use sp_engine::errors::{EngineError, is_write_refusal, plain_reason, unwritable_error};
use sp_engine::library::{
    adopt_sidecar, existing_library_output, in_script_folder, library_output, library_root,
};
use sp_engine::library_images::{
    image_folder_warnings, images_dir_for, prune_images, read_images, write_images,
};
use sp_engine::nodepath::host;
use sp_engine::settings::app::app_settings_path;
use sp_engine::settings::app_defaults::{app_default_options, keeps_script_settings};
use sp_engine::settings::sidecar::{read_script_settings, save_script_settings, sidecar_path};
use sp_engine::{fsx, jsjson};
use sp_model::{FormatOptions, resolve_format_options};

use crate::args::{self, Spec, flag, short, string};
use crate::out::{Io, Run, Stop};
use crate::usage::USAGE;
use crate::verbs::VERBS;

const SPECS: &[Spec] = &[
    short(string("output"), 'o'),
    flag("library"),
    string("fountain"),
    flag("no-fountain"),
    string("title"),
    string("author"),
    flag("force"),
    flag("mobi"),
    string("preview-html"),
    flag("preview-inline"),
    string("fdx"),
    string("azw3"),
    string("options"),
    string("options-json"),
    flag("json"),
    flag("progress"),
    flag("debug"),
    short(flag("help"), 'h'),
    flag("version"),
];

/// `verbHint(input)`: flag-before-verb reaches the conversion; say so.
fn verb_hint(input: &str) -> String {
    if !host::extname(input).is_empty()
        || !VERBS.contains(&input)
        || std::path::Path::new(input).exists()
    {
        return String::new();
    }
    format!(". Did you mean `method-reader {input}`? The verb must come first")
}

/// `checkOutputPath(flag, path)`: refuse a path that cannot be a file in a
/// folder, before any work is done.
fn check_output_path(flag: &str, path: &str) -> Run {
    let dir = host::dirname(path);
    match std::fs::metadata(&dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(Stop::usage(format!(
                "{flag} needs a folder to write into, and {dir} is a file"
            )));
        }
        Err(e) => {
            let code = sp_engine::errors::node_code(&e);
            if code == Some("ENOENT") || code == Some("ENOTDIR") {
                return Err(Stop::usage(format!(
                    "the folder for {flag} does not exist: {dir}. Make it first, or choose a folder that exists"
                )));
            }
            return Ok(());
        }
    }
    if std::fs::metadata(path).is_ok_and(|m| m.is_dir()) {
        return Err(Stop::usage(format!(
            "{flag} needs a file name, and {path} is a folder"
        )));
    }
    Ok(())
}

/// `writeFailed(path, err)`: a refused write is `unwritable`, named for
/// `path` (never the temp file); anything else is the catch-all's.
fn write_failed(path: &str, err: &EngineError) -> Stop {
    if !is_write_refusal(err) {
        return Stop::Internal(format!("Error: {}", err.message()));
    }
    let reason = fsx::hide_temp(&plain_reason(err), path);
    Stop::fail("unwritable", format!("could not write {path}: {reason}"))
}

/// `writeOutput(path, data)`: atomic, or the run ends as `unwritable`.
fn write_output(path: &str, data: &[u8]) -> Run {
    fsx::write_atomic(path, data).map_err(|e| write_failed(path, &e))
}

/// JavaScript truthiness of a parsed JSON value.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// `Math.round(x * 100)`: half toward +infinity.
fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// The ticks, throttled to whole percents (see cli.ts's onProgress).
struct Ticks {
    last_percent: f64,
    last_stage: Option<ConvertStage>,
}

impl Ticks {
    fn line(
        &mut self,
        stage: ConvertStage,
        fraction: f64,
        reading: Option<(u32, u32)>,
    ) -> Option<String> {
        let percent = js_round(fraction * 100.0);
        let last = reading.is_some_and(|(p, n)| p == n);
        if percent == self.last_percent && Some(stage) == self.last_stage && !last {
            return None;
        }
        self.last_percent = percent;
        self.last_stage = Some(stage);
        let mut m = Map::new();
        m.insert("stage".into(), Value::String(stage.as_str().into()));
        m.insert("percent".into(), num(percent));
        if let Some((page, pages)) = reading {
            m.insert("page".into(), Value::from(page));
            m.insert("pages".into(), Value::from(pages));
        }
        Some(jsjson::stringify(
            &serde_json::json!({ "progress": Value::Object(m) }),
        ))
    }
}

fn num(x: f64) -> Value {
    serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
}

/// Where a conversion error goes: the two refusals and the PDF's own
/// failures by their codes, anything else through the catch-all.
fn conversion_failed(err: ConvertError) -> Stop {
    match err {
        ConvertError::Scanned { .. } => Stop::fail("scanned", err.to_string()),
        ConvertError::NotAScreenplay => Stop::fail("not-screenplay", err.to_string()),
        ConvertError::Password => Stop::fail(
            "password",
            "this PDF is password-protected: remove the password first",
        ),
        ConvertError::InvalidPdf => Stop::fail(
            "unreadable",
            "this file is not a readable PDF. It may be corrupt or mislabeled",
        ),
        ConvertError::NotUtf8 | ConvertError::EmptyScript => {
            Stop::fail("unreadable", err.to_string())
        }
        ConvertError::Fountain(m) => Stop::Internal(format!("TypeError: {m}")),
        ConvertError::Book(m) => Stop::internal(m),
    }
}

/// `mapConversionError`'s filesystem half, for the input read.
fn read_failed(err: &EngineError) -> Stop {
    match sp_engine::errors::map_read_error(err) {
        Some(e) => Stop::fail(e.code.as_str(), e.message),
        None => Stop::Internal(format!("Error: {}", err.message())),
    }
}

pub fn run(io: &mut Io, argv: &[String]) -> Run {
    let errs = io.err_line();
    let eprint_line = |text: &str| errs.line(text);
    let parsed = args::parse(argv, SPECS).map_err(Stop::usage)?;
    io.json = parsed.flag("json");
    let json = io.json;

    if parsed.flag("version") {
        let version = env!("CARGO_PKG_VERSION");
        if json {
            io.ok(serde_json::json!({ "version": version }));
        } else {
            io.say(&format!("method-reader {version} (Method Reader)"));
        }
        return Err(Stop::Exit(0));
    }
    if parsed.flag("help") {
        return Err(io.help(USAGE));
    }
    let pos = &parsed.positionals;
    if pos.is_empty() {
        if json {
            return Err(Stop::usage("expected exactly one input file (see --help)"));
        }
        io.say(USAGE);
        return Err(Stop::Exit(1));
    }
    if pos.len() > 1 {
        return Err(Stop::usage("expected exactly one input file"));
    }
    let preview_inline = parsed.flag("preview-inline");
    if preview_inline && !json {
        return Err(Stop::usage(
            "--preview-inline needs --json: the document rides inside the result object",
        ));
    }

    let input = pos[0].as_str();
    let ext = host::extname(input).to_lowercase();
    let is_pdf = ext == ".pdf";
    let is_fountain = ext == ".fountain" || ext == ".txt";
    let fountain_flag = parsed.string("fountain");
    let no_fountain = parsed.flag("no-fountain");
    let output = parsed.string("output");
    let library = parsed.flag("library");
    if fountain_flag.is_some() && no_fountain {
        return Err(Stop::usage(
            "pass --fountain or --no-fountain, not both (--no-fountain writes no .fountain for --fountain to name)",
        ));
    }
    if fountain_flag.is_some() && is_fountain {
        return Err(Stop::usage(format!(
            "a {ext} input takes no --fountain (only a PDF conversion writes a .fountain)"
        )));
    }
    if library && output.is_some() {
        return Err(Stop::usage(
            "pass --library or -o, not both: -o already says where the output goes",
        ));
    }
    let preview_html = parsed.string("preview-html").filter(|s| !s.is_empty());
    let fdx = parsed.string("fdx").filter(|s| !s.is_empty());
    let azw3 = parsed.string("azw3").filter(|s| !s.is_empty());
    if is_pdf || is_fountain {
        if let Some(o) = output {
            check_output_path("-o", o)?;
        }
        if let Some(f) = fountain_flag {
            check_output_path("--fountain", f)?;
        }
        if let Some(p) = preview_html {
            check_output_path("--preview-html", p)?;
        }
        if let Some(p) = fdx {
            check_output_path("--fdx", p)?;
        }
        if let Some(p) = azw3 {
            check_output_path("--azw3", p)?;
        }
    }
    let options_file = parsed.string("options");
    let options_json = parsed.string("options-json");
    if options_file.is_some() && options_json.is_some() {
        return Err(Stop::fail(
            "bad-options",
            "pass --options or --options-json, not both",
        ));
    }
    let mut format: Option<Value> = None;
    if let Some(file) = options_file.filter(|f| !f.is_empty()) {
        let parsed = fsx::read_text(file).ok().and_then(|t| jsjson::parse(&t));
        match parsed {
            Some(v) => format = Some(v),
            None => {
                return Err(Stop::fail(
                    "bad-options",
                    format!("cannot read options file {file}"),
                ));
            }
        }
    }
    if let Some(text) = options_json {
        let Some(v) = jsjson::parse(text) else {
            return Err(Stop::fail(
                "bad-options",
                "--options-json is not valid JSON",
            ));
        };
        if !v.is_object() {
            return Err(Stop::fail(
                "bad-options",
                "--options-json must be a JSON object",
            ));
        }
        format = Some(v);
    }

    // The settings layers: the flags over the script's own sidecar over the
    // app defaults over the shipped ones, read once per conversion.
    let platform = host_platform();
    let env = Env::process();
    let settings_file = app_settings_path(platform, &env);
    let app_defaults = app_default_options(&settings_file);
    let keep_settings = library && keeps_script_settings(&settings_file);
    let mut settings: Option<FormatOptions> = None;
    let mut settings_path: Option<String> = None;
    let mut candidates: Vec<String> = Vec::new();
    let mut lib_root: Option<String> = None;
    if library {
        let root = library_root(platform, &env, Some(&settings_file));
        if let Ok(Some(prefix)) = existing_library_output(input, &root) {
            candidates.push(format!("{prefix}.fountain"));
        }
        lib_root = Some(root);
    }
    candidates.push(input.to_owned());
    for candidate in &candidates {
        let Some(read) = read_script_settings(candidate, &app_defaults) else {
            continue;
        };
        let Some(s) = read.settings else {
            eprint_line(&format!(
                "method-reader: ignoring {}, which is not a settings object",
                read.path
            ));
            continue;
        };
        eprint_line(&format!(
            "method-reader: using this script's saved settings in {}{}",
            read.path,
            if format.as_ref().is_some_and(truthy) {
                " (the options you passed override them)"
            } else {
                ""
            }
        ));
        settings = Some(s);
        settings_path = Some(read.path);
        break;
    }
    let format_for_convert =
        resolve_format_options(format.as_ref(), settings.as_ref().unwrap_or(&app_defaults));

    let progress = parsed.flag("progress");
    let mut ticks = Ticks {
        last_percent: -1.0,
        last_stage: None,
    };
    let mut on_progress = |stage: ConvertStage, fraction: f64, reading: Option<(u32, u32)>| {
        if let Some(line) = ticks.line(stage, fraction, reading) {
            eprint_line(&line);
        }
    };
    let mut on_first_page = |page: u32, paragraphs: Vec<String>| {
        eprint_line(&jsjson::stringify(&serde_json::json!({
            "progress": { "stage": "page", "page": page, "paragraphs": paragraphs }
        })));
    };
    let mobi = parsed.flag("mobi");
    let opts = ConvertOptions {
        title: parsed.string("title").map(str::to_owned),
        author: parsed.string("author").map(str::to_owned),
        force: parsed.flag("force"),
        mobi,
        mobi_pictures: None,
        format: format_for_convert,
        on_progress: if progress {
            Some(&mut on_progress)
        } else {
            None
        },
        on_first_page: if progress {
            Some(&mut on_first_page)
        } else {
            None
        },
        picture_bytes: None,
    };

    let mut carried: Vec<(String, Vec<u8>)> = Vec::new();
    let result: ConvertResult = if is_pdf {
        let bytes = fsx::read(input).map_err(|e| read_failed(&e))?;
        sp_engine::convert::convert_pdf(bytes, opts).map_err(conversion_failed)?
    } else if is_fountain {
        let bytes = fsx::read(input).map_err(|e| read_failed(&e))?;
        let text = sp_engine::convert::fountain_text(bytes).map_err(conversion_failed)?;
        // The pictures the notes name, from the folder beside the text; not
        // read at all with pictures off.
        let mut asked: BTreeSet<String> = BTreeSet::new();
        let pictures = if format_for_convert.include_images {
            asked = sp_fountain::image_note_files(&text)
                .map_err(|e| Stop::Internal(format!("TypeError: {e}")))?
                .into_iter()
                .collect();
            Some(read_images(input, &asked))
        } else {
            None
        };
        let images = pictures.as_ref().map(|p| &p.images);
        // A script someone chose is held to the .fountain guards; one the
        // library made (the window renders it again on every tune) is not.
        let mut r = if in_script_folder(input) {
            sp_engine::convert::convert_fountain(&text, opts, images)
        } else {
            sp_engine::convert::convert_chosen_fountain(&text, opts, images)
        }
        .map_err(conversion_failed)?;
        if let Some(p) = &pictures {
            r.warnings.extend(image_folder_warnings(p));
        }
        // What a library copy of this script carries with it: the pictures
        // its notes name, as a PDF's copy carries only its own.
        if library {
            carried = sp_engine::convert::carried_pictures(images)
                .into_iter()
                .filter(|i| asked.contains(&i.name))
                .map(|i| (i.name, i.bytes))
                .collect();
        }
        r
    } else {
        return Err(Stop::fail(
            "unsupported-type",
            format!(
                "unsupported input type \"{ext}\": expected .pdf, .fountain, or .txt{}",
                verb_hint(input)
            ),
        ));
    };
    finish(
        io,
        &parsed,
        input,
        is_pdf,
        result,
        Finish {
            library,
            lib_root,
            keep_settings,
            app_defaults,
            settings_path,
            preview_inline,
            preview_html,
            fdx,
            azw3,
            carried,
        },
    )
}

/// `saveScriptSettings` for the pin. A debug build (the one the tests run)
/// fails it on demand, with METHOD_READER_TEST_FAIL_PIN set, standing for the
/// TypeScript test's preload that blocks only the sidecar's temp file: no
/// filesystem trick fails that one write and not the book's. A release
/// build has no such switch.
fn save_pin(settings: &FormatOptions, stem: &str) -> Result<(), EngineError> {
    if cfg!(debug_assertions) && std::env::var_os("METHOD_READER_TEST_FAIL_PIN").is_some() {
        return Err(EngineError::plain("EACCES: permission denied (test)"));
    }
    save_script_settings(settings, stem)
}

struct Finish<'a> {
    library: bool,
    lib_root: Option<String>,
    keep_settings: bool,
    app_defaults: FormatOptions,
    settings_path: Option<String>,
    preview_inline: bool,
    preview_html: Option<&'a str>,
    fdx: Option<&'a str>,
    azw3: Option<&'a str>,
    /// A .fountain input's pictures, for its copy in the library.
    carried: Vec<(String, Vec<u8>)>,
}

/// Everything after the conversion: where the output goes (decided only
/// now, so a refusal leaves the library as it was), each write, the pin,
/// and the answer.
fn finish(
    io: &mut Io,
    parsed: &args::Parsed,
    input: &str,
    is_pdf: bool,
    result: ConvertResult,
    f: Finish,
) -> Run {
    let errs = io.err_line();
    let eprint_line = |text: &str| errs.line(text);
    let mut input_stem = host::join(&[
        host::dirname(input),
        host::basename_ext(input, &host::extname(input)),
    ]);
    if f.library {
        let root = f.lib_root.unwrap_or_default();
        let opened = library_output(input, &root).and_then(|stem| {
            adopt_sidecar(input, &stem)?;
            Ok(stem)
        });
        match opened {
            Ok(stem) => input_stem = stem,
            Err(e) => {
                return Err(Stop::fail(
                    "library",
                    format!("cannot open the library folder: {}", e.message()),
                ));
            }
        }
    }
    let epub_path = parsed
        .string("output")
        .map_or_else(|| format!("{input_stem}.epub"), str::to_owned);
    let stem = host::join(&[
        host::dirname(&epub_path),
        host::basename_ext(&epub_path, &host::extname(&epub_path)),
    ]);

    write_output(&epub_path, &result.epub)?;

    let mut mobi_path = None;
    if parsed.flag("mobi")
        && let Some(mobi) = &result.mobi
    {
        let p = format!("{stem}.mobi");
        write_output(&p, mobi)?;
        mobi_path = Some(p);
    }
    let mut azw3_path = None;
    if let Some(p) = f.azw3 {
        let book = sp_azw3::build_azw3(&result.epub).map_err(Stop::internal)?;
        write_output(p, &book)?;
        azw3_path = Some(p.to_owned());
    }

    let mut fountain_path = None;
    let mut images_path = None;
    // A PDF's .fountain is written wherever the book goes (unless asked
    // not to). A .fountain input is already one, so it is written only
    // into the library, where the script's folder keeps its own copy (and
    // its pictures) the way it keeps a PDF's, for the window to tune and
    // export from. Never over the input itself.
    let fountain_out = if is_pdf && !parsed.flag("no-fountain") {
        Some((
            parsed
                .string("fountain")
                .map_or_else(|| format!("{stem}.fountain"), str::to_owned),
            result
                .images
                .iter()
                .map(|i| (i.name.clone(), i.bytes.clone()))
                .collect::<Vec<(String, Vec<u8>)>>(),
        ))
    } else if !is_pdf && f.library && !parsed.flag("no-fountain") {
        let fp = format!("{stem}.fountain");
        (host::resolve(&[fp.as_str()]) != host::resolve(&[input])).then_some((fp, f.carried))
    } else {
        None
    };
    let stored_fountain = !is_pdf && fountain_out.is_some();
    if let Some((fp, pictures)) = fountain_out {
        let dir = images_dir_for(&fp);
        if pictures.is_empty() {
            prune_images(&dir, &BTreeSet::new());
        } else {
            if let Err(err) = write_images(&dir, &pictures) {
                let already: Vec<String> =
                    [Some(&epub_path), mobi_path.as_ref(), azw3_path.as_ref()]
                        .into_iter()
                        .flatten()
                        .cloned()
                        .collect();
                return Err(match unwritable_error(&dir, &err, &already) {
                    Some(e) => Stop::fail(e.code.as_str(), e.message),
                    None => Stop::Internal(format!("Error: {}", err.message())),
                });
            }
            images_path = Some(dir);
        }
        write_output(&fp, result.fountain_text.as_bytes())?;
        // The scene lengths the PDF measured, kept in the script folder's
        // source.json for a later FDX export (nothing is kept outside the
        // library). Best effort: the book is already made.
        if f.library
            && let Err(e) = sp_engine::library::save_scene_measures(
                &fp,
                &result.fountain_text,
                result.scene_measures.as_deref(),
            )
        {
            eprint_line(&format!(
                "method-reader: could not keep the scene lengths beside {fp}: {}",
                e.message()
            ));
        }
        fountain_path = Some(fp);
    }
    let mut preview_path = None;
    if let Some(p) = f.preview_html {
        write_output(p, result.preview_html.as_bytes())?;
        preview_path = Some(p.to_owned());
    }
    let mut fdx_path = None;
    if let Some(p) = f.fdx {
        let tokens = sp_fountain::tokenize(&result.fountain_text)
            .map_err(|e| Stop::Internal(format!("TypeError: {e}")))?;
        let fdx = sp_fdx::tokens_to_fdx_measured(&tokens, result.scene_measures.as_deref());
        write_output(p, fdx.as_bytes())?;
        fdx_path = Some(p.to_owned());
    }
    let mut debug_path = None;
    if parsed.flag("debug")
        && let Some(sp) = &result.screenplay
    {
        let p = format!("{stem}.elements.json");
        let text = jsjson::stringify_pretty(&serde_json::to_value(sp).expect("serializes"));
        // In place, not through a temp file; only a failure is reported the
        // same way.
        fsx::write(&p, text.as_bytes()).map_err(|e| write_failed(&p, &e))?;
        debug_path = Some(p);
    }

    // The pin: what an untuned script started from becomes its own the
    // first time it lands in the library (a PDF, or a .fountain that now
    // has its copy there; never over a sidecar that is there).
    if f.keep_settings && (is_pdf || stored_fountain) {
        let pin = sidecar_path(&input_stem);
        if !std::path::Path::new(&pin).exists()
            && let Err(e) = save_pin(&f.app_defaults, &input_stem)
        {
            eprint_line(&format!(
                "method-reader: could not save this script's settings to {pin}: {}",
                e.message()
            ));
        }
    }

    // The suite launcher's recent list (sp_engine::recent decides what
    // counts): after every write, and never a reason to fail. A damaged
    // file set aside is said here, once, by the run that set it aside.
    match sp_engine::recent::record_conversion(
        host_platform(),
        &Env::process(),
        input,
        f.library,
        sp_engine::recent::now_seconds(),
    ) {
        Ok(Some(sp_engine::recent::Recorded {
            damaged: Some(aside),
        })) => eprint_line(&format!(
            "method-reader: {} was damaged, so it was kept as {aside} and a new list was started",
            sp_engine::recent::RECENT_FILE
        )),
        Ok(_) => {}
        Err(e) => eprint_line(&format!(
            "method-reader: could not add this script to {}: {}",
            sp_engine::recent::RECENT_FILE,
            e.message()
        )),
    }

    let sp = result.screenplay.as_ref();
    if io.json {
        let mut m = Map::new();
        m.insert("ok".into(), Value::Bool(true));
        m.insert("title".into(), Value::String(result.title.clone()));
        let mut put = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                m.insert(k.into(), v);
            }
        };
        put("author", result.author.clone().map(Value::String));
        put("pages", sp.map(|s| Value::from(s.page_count)));
        put("scenes", sp.map(|s| Value::from(s.scenes.len())));
        put("characters", sp.map(|s| Value::from(s.characters.len())));
        put(
            "topCharacters",
            Some(Value::Array(
                sp.map(|s| {
                    s.characters
                        .iter()
                        .take(5)
                        .map(|c| Value::String(c.name.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            )),
        );
        put(
            "warnings",
            Some(Value::Array(
                result.warnings.iter().cloned().map(Value::String).collect(),
            )),
        );
        put("spacingRepairs", Some(Value::from(result.spacing_repairs)));
        let path = |p: &Option<String>| p.clone().map(Value::String);
        put("epubPath", Some(Value::String(epub_path.clone())));
        put("mobiPath", path(&mobi_path));
        put("azw3Path", path(&azw3_path));
        put("fountainPath", path(&fountain_path));
        put("imagesPath", path(&images_path));
        put(
            "imageCount",
            (!result.images.is_empty()).then(|| Value::from(result.images.len())),
        );
        put("settingsPath", path(&f.settings_path));
        put("previewHtmlPath", path(&preview_path));
        put("fdxPath", path(&fdx_path));
        put("debugPath", path(&debug_path));
        if f.preview_inline {
            put(
                "previewHtml",
                Some(Value::String(result.preview_html.clone())),
            );
        }
        io.say(&jsjson::stringify(&Value::Object(m)));
        return Ok(());
    }

    io.say(&match &result.author {
        Some(a) if !a.is_empty() => format!("{}, by {a}", result.title),
        _ => result.title.clone(),
    });
    if let Some(sp) = sp {
        let top: Vec<&str> = sp
            .characters
            .iter()
            .take(5)
            .map(|c| c.name.as_str())
            .collect();
        let top = top.join(", ");
        io.say(&format!(
            "  {} pages · {} scenes · {} speaking characters{}",
            sp.page_count,
            sp.scenes.len(),
            sp.characters.len(),
            if top.is_empty() {
                String::new()
            } else {
                format!(" ({top}…)")
            }
        ));
    }
    for w in &result.warnings {
        io.say(&format!("  warning: {w}"));
    }
    for p in [
        Some(epub_path),
        mobi_path,
        azw3_path,
        fountain_path,
        images_path,
        preview_path,
        fdx_path,
        debug_path,
    ]
    .into_iter()
    .flatten()
    {
        if !p.is_empty() {
            io.say(&format!("  wrote {p}"));
        }
    }
    Ok(())
}
