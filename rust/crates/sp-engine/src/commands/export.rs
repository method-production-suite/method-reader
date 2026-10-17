//! Port of `src/cli-export.ts`: the `export` verb. The file you would
//! actually put on a reader. The LADDER is artifact.rs's; this chooses
//! nothing, it only checks the request and carries the answer out.

use serde::Serialize;
use serde_json::{Map, Value};
use sp_model::{FormatOptions, resolve_format_options};

use crate::env::{Env, host_platform};
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::export::artifact::{
    BuiltBy, FreshKindleArtifactOptions, RealBuilders, RungBuilders, available_formats,
    fresh_artifact_of, fresh_kindle_artifact, kindle_artifact_plan, rung_plan,
};
use crate::export::calibre::{is_calibre_available, strip_epub};
use crate::export::formats::{
    ExportFormat, KindleRung, ToolchainState, file_extension, format_label, is_beta_file,
};
use crate::export::kfx::{KfxStatus, kfx_missing, kfx_status};
use crate::export::renderers::Renderers;
use crate::fsx;
use crate::jsjson;
use crate::nodepath::host;
use crate::settings::app::app_settings_path;
use crate::settings::app_defaults::app_default_options;
use crate::settings::sidecar::load_script_settings;

/// `ExportResult`. Serialized in the TypeScript's field order, which is
/// not one order: the FDX answer lists `available` last.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportResult {
    pub path: String,
    pub format: ExportFormat,
    pub extension: String,
    pub label: String,
    pub available: Vec<ExportFormat>,
    pub stages: Vec<String>,
    /// --check only: whether the file at `path` is current.
    pub fresh: Option<bool>,
    /// --check on a Kindle rung only: what builds it.
    pub built_by: Option<BuiltBy>,
    /// --check on a Kindle rung only: when the EPUB was last written (ms).
    pub book_date: Option<Option<f64>>,
    /// True when the file is one Method Reader still calls beta.
    pub beta: bool,
    /// The FDX answer's own order: `available` after `stages` and `fresh`.
    available_last: bool,
}

impl ExportResult {
    /// The answer as the CLI prints it (without `ok`).
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        let available = serde_json::to_value(&self.available).expect("formats serialize");
        m.insert("path".into(), Value::String(self.path.clone()));
        m.insert("format".into(), Value::String(self.format.as_str().into()));
        m.insert("extension".into(), Value::String(self.extension.clone()));
        m.insert("label".into(), Value::String(self.label.clone()));
        if !self.available_last {
            m.insert("available".into(), available.clone());
        }
        m.insert(
            "stages".into(),
            Value::Array(self.stages.iter().cloned().map(Value::String).collect()),
        );
        if let Some(fresh) = self.fresh {
            m.insert("fresh".into(), Value::Bool(fresh));
        }
        if let Some(built_by) = self.built_by {
            m.insert(
                "builtBy".into(),
                serde_json::to_value(built_by).expect("serializes"),
            );
        }
        if let Some(date) = self.book_date {
            m.insert(
                "bookDate".into(),
                date.and_then(serde_json::Number::from_f64)
                    .map_or(Value::Null, Value::Number),
            );
        }
        if self.available_last {
            m.insert("available".into(), available);
        }
        if self.beta {
            m.insert("beta".into(), Value::Bool(true));
        }
        Value::Object(m)
    }
}

impl Serialize for ExportResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(s)
    }
}

/// `ExportOptions`.
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// An EXISTING .epub.
    pub epub: String,
    /// One of EXPORT_FORMATS; `epub` when absent.
    pub for_: Option<String>,
    /// The script's .fountain.
    pub fountain: Option<String>,
    /// This script's settings, as JSON.
    pub options_json: Option<String>,
    /// Where to put a copy: absolute.
    pub out: Option<String>,
    /// Say which file and whether it is current; build nothing.
    pub check: bool,
}

/// The Kindle ladder as one call (`freshKindleArtifact`), replaceable.
pub type KindleLadder<'a> =
    dyn Fn(&FreshKindleArtifactOptions, &mut dyn FnMut(&str)) -> Result<String, EngineError> + 'a;

/// `ExportDeps`: every seam, each defaulting to the real thing.
#[derive(Default)]
pub struct ExportDeps<'a> {
    pub calibre_available: Option<Box<dyn Fn() -> bool + 'a>>,
    pub kfx_status: Option<Box<dyn Fn() -> KfxStatus + 'a>>,
    pub fresh_kindle_artifact: Option<Box<KindleLadder<'a>>>,
    pub rung_builders: Option<&'a dyn RungBuilders>,
    /// The app settings file the defaults are read from (the production
    /// one when absent).
    pub app_settings_path: Option<String>,
    /// The environment for every probe and Calibre spawn (this process's
    /// when absent). A test MUST give a scratch CALIBRE_CONFIG_DIRECTORY.
    pub env: Option<Env>,
}

fn usage(message: impl Into<String>) -> EngineError {
    CliError::new(ErrorCode::Usage, message).into()
}

fn export_failed(message: impl Into<String>) -> EngineError {
    CliError::new(ErrorCode::ExportFailed, message).into()
}

fn unreadable(message: impl Into<String>) -> EngineError {
    CliError::new(ErrorCode::Unreadable, message).into()
}

/// `readFormatBase`: the script's own tuning when export knows the script,
/// else the app defaults.
fn read_format_base(fountain: Option<&str>, app_settings: &str) -> FormatOptions {
    let app_defaults = app_default_options(app_settings);
    match fountain {
        None => app_defaults,
        Some(f) => load_script_settings(f, &app_defaults),
    }
}

/// `readFormat`: `--options-json` overlaid on `base`.
fn read_format(
    options_json: Option<&str>,
    base: FormatOptions,
) -> Result<FormatOptions, EngineError> {
    let Some(text) = options_json else {
        return Ok(base);
    };
    let Some(parsed) = jsjson::parse(text) else {
        return Err(
            CliError::new(ErrorCode::BadOptions, "--options-json is not valid JSON").into(),
        );
    };
    if !matches!(parsed, Value::Object(_)) {
        return Err(CliError::new(
            ErrorCode::BadOptions,
            "--options-json must be a JSON object",
        )
        .into());
    }
    Ok(resolve_format_options(Some(&parsed), &base))
}

/// `checkOutExtension(out, wantExt)`.
fn check_out_extension(out: &str, want_ext: &str) -> Result<(), EngineError> {
    let out_ext = host::extname(out);
    let out_ext = out_ext.strip_prefix('.').unwrap_or(&out_ext);
    if out_ext.to_lowercase() != want_ext.to_lowercase() {
        let kind = want_ext.to_uppercase();
        let article = if kind.starts_with(['A', 'E', 'I', 'O', 'U']) {
            "an"
        } else {
            "a"
        };
        return Err(usage(format!(
            "that is {article} {kind} file: choose a name ending in .{}",
            want_ext.to_lowercase()
        )));
    }
    Ok(())
}

/// `copyArtifactAtomic(source, destination)`: temp file in the destination
/// folder, then rename over.
fn copy_artifact_atomic(source: &str, destination: &str) -> Result<(), EngineError> {
    fsx::mkdir_all(&host::dirname(destination))?;
    let tmp = fsx::temp_path_for(destination);
    let result = fsx::copy(source, &tmp).and_then(|()| fsx::rename(&tmp, destination));
    if result.is_err() {
        fsx::rm_force(&tmp);
    }
    result
}

/// `resolveOut(artifactPath, out)`: copy the artifact to `out` unless it is
/// already there; a failure names `out`, never the hidden temp file.
fn resolve_out(artifact_path: &str, out: &str) -> Result<String, EngineError> {
    if host::resolve(&[out]) != host::resolve(&[artifact_path])
        && let Err(err) = copy_artifact_atomic(artifact_path, out)
    {
        let reason = fsx::hide_temp(&err.message(), out);
        return Err(export_failed(format!(
            "could not save the copy to {out}: {reason}"
        )));
    }
    Ok(out.to_owned())
}

/// `checkScriptReadable(script)`.
fn check_script_readable(script: &str) -> Result<(), EngineError> {
    if fsx::is_file(script) {
        Ok(())
    } else {
        Err(unreadable(format!(
            "cannot read the script to export: {script}"
        )))
    }
}

/// `saveFountainCopy(script, out)`: the script without Method Reader's own
/// notes; the script beside the book is never written.
fn save_fountain_copy(
    script: &str,
    out: &str,
    renderers: &dyn Renderers,
) -> Result<String, EngineError> {
    let text = fsx::read_text(script)
        .map_err(|_| unreadable(format!("cannot read the script to export: {script}")))?;
    let clean = renderers
        .strip_screepub_notes(&text)
        .map_err(EngineError::plain)?;
    if clean == text {
        return resolve_out(script, out);
    }
    if host::resolve(&[out]) == host::resolve(&[script]) {
        return Err(export_failed(format!(
            "could not save the copy to {out}: that is the book's own script, which Method Reader keeps; choose another place"
        )));
    }
    if let Err(err) =
        fsx::mkdir_all(&host::dirname(out)).and_then(|()| fsx::write_atomic(out, clean.as_bytes()))
    {
        let reason = fsx::hide_temp(&err.message(), out);
        return Err(export_failed(format!(
            "could not save the copy to {out}: {reason}"
        )));
    }
    Ok(out.to_owned())
}

/// `fdxSibling(epub)`.
pub fn fdx_sibling(epub: &str) -> String {
    format!("{}.fdx", strip_epub(epub))
}

fn result(
    path: String,
    format: ExportFormat,
    state: ToolchainState,
    available: Vec<ExportFormat>,
    stages: Vec<String>,
) -> ExportResult {
    ExportResult {
        path,
        format,
        extension: file_extension(format, state).to_owned(),
        label: format_label(format, state),
        available,
        stages,
        fresh: None,
        built_by: None,
        book_date: None,
        beta: false,
        available_last: false,
    }
}

/// `exportFdx`: the editable screenplay, rebuilt from the .fountain on
/// every call and written atomically.
fn export_fdx(
    epub: &str,
    fountain_path: &str,
    out: Option<&str>,
    check: bool,
    available: Vec<ExportFormat>,
    renderers: &dyn Renderers,
) -> Result<ExportResult, EngineError> {
    let state = ToolchainState::default();
    let extension = file_extension(ExportFormat::Fdx, state);
    if let Some(out) = out {
        check_out_extension(out, extension)?;
    }
    let path = out.map_or_else(|| fdx_sibling(epub), str::to_owned);

    let read = fsx::read_text(fountain_path)
        .ok()
        .zip(fsx::mtime_ms(fountain_path));
    let Some((fountain_text, fountain_date)) = read else {
        return Err(unreadable(format!(
            "cannot read the script's .fountain: {fountain_path}"
        )));
    };
    let mut base = result(
        path.clone(),
        ExportFormat::Fdx,
        state,
        available,
        Vec::new(),
    );
    base.available_last = true;

    if check {
        base.fresh = Some(fsx::mtime_ms(&path).is_some_and(|m| m >= fountain_date));
        return Ok(base);
    }

    let written = renderers
        .fountain_to_fdx(&fountain_text)
        .map_err(EngineError::plain)
        .and_then(|fdx| {
            fsx::mkdir_all(&host::dirname(&path))?;
            fsx::write_atomic(&path, fdx.as_bytes())
        });
    if let Err(err) = written {
        let reason = fsx::hide_temp(&err.message(), &path);
        return Err(export_failed(format!(
            "could not save the FDX to {path}: {reason}"
        )));
    }
    Ok(base)
}

/// `exportCommand(options, deps)`.
pub fn export_command(
    options: &ExportOptions,
    deps: &ExportDeps,
    renderers: &dyn Renderers,
) -> Result<ExportResult, EngineError> {
    let mut answer = export_file(options, deps, renderers)?;
    if is_beta_file(&answer.extension) {
        answer.beta = true;
    }
    Ok(answer)
}

fn export_file(
    options: &ExportOptions,
    deps: &ExportDeps,
    renderers: &dyn Renderers,
) -> Result<ExportResult, EngineError> {
    let wanted = options.for_.as_deref().unwrap_or("epub");
    let Some(format) = ExportFormat::parse(wanted) else {
        return Err(usage(format!(
            "--for takes epub, kindle, kfx, azw3, mobi, fountain or fdx, not \"{wanted}\""
        )));
    };

    if !fsx::is_file(&options.epub) {
        return Err(unreadable(format!(
            "cannot read the book to export: {}",
            options.epub
        )));
    }

    if options.check && options.out.is_some() {
        return Err(usage(
            "--check builds and writes nothing, so it takes no --out",
        ));
    }
    if let Some(out) = &options.out {
        if !host::is_absolute(out) {
            return Err(usage("the path given with --out must be absolute"));
        }
        if format != ExportFormat::Epub && format != ExportFormat::Kindle {
            check_out_extension(out, format.as_str())?;
        }
    }
    let needs_script = matches!(format, ExportFormat::Mobi | ExportFormat::Fountain);
    if needs_script && options.fountain.is_none() {
        return Err(usage(format!(
            "--for {} needs the script's .fountain: pass it with --fountain",
            format.as_str()
        )));
    }
    if format == ExportFormat::Fountain {
        let script = options.fountain.as_deref().unwrap_or_default();
        if host::extname(script).to_lowercase() != ".fountain" {
            return Err(usage(format!(
                "--for fountain takes a .fountain file, not {script}"
            )));
        }
    }
    if format == ExportFormat::Fdx && options.fountain.is_none() {
        return Err(usage(
            "--for fdx is built from the script's .fountain: pass it with --fountain",
        ));
    }
    if matches!(
        format,
        ExportFormat::Mobi | ExportFormat::Fountain | ExportFormat::Fdx
    ) {
        check_script_readable(options.fountain.as_deref().unwrap_or_default())?;
    }

    let env = deps.env.clone().unwrap_or_else(Env::process);
    let settings_file = deps
        .app_settings_path
        .clone()
        .unwrap_or_else(|| app_settings_path(host_platform(), &env));
    let format_base = read_format_base(options.fountain.as_deref(), &settings_file);
    let format_options = read_format(options.options_json.as_deref(), format_base)?;

    let calibre_available = match &deps.calibre_available {
        Some(f) => f(),
        None => is_calibre_available(&env),
    };
    let available = available_formats();
    let check = options.check;

    if format == ExportFormat::Fountain {
        let script = options.fountain.clone().unwrap_or_default();
        let state = ToolchainState {
            calibre_available,
            kfx_ready: false,
        };
        let path = match &options.out {
            Some(out) => save_fountain_copy(&script, out, renderers)?,
            None => script,
        };
        let mut r = result(path, format, state, available, Vec::new());
        if check {
            r.fresh = Some(true);
        }
        return Ok(r);
    }

    let real_builders;
    let builders: &dyn RungBuilders = match deps.rung_builders {
        Some(b) => b,
        None => {
            real_builders = RealBuilders {
                env: &env,
                renderers,
            };
            &real_builders
        }
    };
    let kfx = || match &deps.kfx_status {
        Some(f) => f(),
        None => kfx_status(&env),
    };

    if let Some(rung) = match format {
        ExportFormat::Kfx => Some(KindleRung::Kfx),
        ExportFormat::Azw3 => Some(KindleRung::Azw3),
        ExportFormat::Mobi => Some(KindleRung::Mobi),
        _ => None,
    } {
        return export_rung(
            rung,
            options,
            &kfx,
            builders,
            renderers,
            calibre_available,
            available,
            format_options,
        );
    }

    if format == ExportFormat::Fdx {
        return export_fdx(
            &options.epub,
            options.fountain.as_deref().unwrap_or_default(),
            options.out.as_deref(),
            check,
            available,
            renderers,
        );
    }

    if format == ExportFormat::Epub {
        let state = ToolchainState {
            calibre_available,
            kfx_ready: false,
        };
        let extension = file_extension(format, state);
        if let Some(out) = &options.out {
            check_out_extension(out, extension)?;
        }
        let path = match &options.out {
            Some(out) => resolve_out(&options.epub, out)?,
            None => options.epub.clone(),
        };
        let mut r = result(path, format, state, available, Vec::new());
        if check {
            r.fresh = Some(true);
        }
        return Ok(r);
    }

    // The ladder (`--for kindle`).
    let status = kfx();
    let state = ToolchainState {
        calibre_available,
        kfx_ready: status.ready,
    };
    if check {
        let plan = kindle_artifact_plan(&options.epub, state, renderers.azw3_writer_revision());
        let mut r = result(plan.path, format, state, available, Vec::new());
        r.fresh = Some(plan.fresh);
        r.built_by = Some(plan.built_by);
        r.book_date = Some(plan.book_date);
        return Ok(r);
    }
    let mut stages: Vec<String> = Vec::new();
    let ladder_opts = FreshKindleArtifactOptions {
        epub: options.epub.clone(),
        fountain_path: options.fountain.clone(),
        format: format_options,
        calibre_available,
        kfx_ready: status.ready,
    };
    let built = {
        let mut on_stage = |s: &str| stages.push(s.to_owned());
        match &deps.fresh_kindle_artifact {
            Some(ladder) => ladder(&ladder_opts, &mut on_stage),
            None => fresh_kindle_artifact(&ladder_opts, builders, renderers, &mut on_stage),
        }
    };
    let mut path = built.map_err(|err| export_failed(err.message()))?;
    let extension = file_extension(format, state);
    if let Some(out) = &options.out {
        check_out_extension(out, extension)?;
        path = resolve_out(&path, out)?;
    }
    Ok(result(path, format, state, available, stages))
}

/// `exportRung`: one rung by name, never the ladder, so a missing
/// toolchain is a refusal and never a quieter rung.
#[allow(clippy::too_many_arguments)]
fn export_rung(
    rung: KindleRung,
    options: &ExportOptions,
    kfx: &dyn Fn() -> KfxStatus,
    builders: &dyn RungBuilders,
    renderers: &dyn Renderers,
    calibre_available: bool,
    available: Vec<ExportFormat>,
    format_options: FormatOptions,
) -> Result<ExportResult, EngineError> {
    let mut kfx_ready = false;
    if rung == KindleRung::Kfx {
        let status = kfx();
        if !status.ready {
            let missing = kfx_missing(&status);
            let why = if missing.is_empty() {
                "its tools are not ready".to_owned()
            } else {
                format!("it needs {}", missing.join(" and "))
            };
            return Err(export_failed(format!(
                "this computer cannot make a KFX file yet: {why}"
            )));
        }
        kfx_ready = true;
    }
    let state = ToolchainState {
        calibre_available,
        kfx_ready,
    };
    let format = rung.format();

    if options.check {
        let plan = rung_plan(rung, &options.epub, renderers.azw3_writer_revision());
        let mut r = result(plan.path, format, state, available, Vec::new());
        r.fresh = Some(plan.fresh);
        r.built_by = Some(plan.built_by);
        r.book_date = Some(plan.book_date);
        return Ok(r);
    }

    let mut stages: Vec<String> = Vec::new();
    let opts = FreshKindleArtifactOptions {
        epub: options.epub.clone(),
        fountain_path: options.fountain.clone(),
        format: format_options,
        calibre_available,
        kfx_ready,
    };
    let built = {
        let mut on_stage = |s: &str| stages.push(s.to_owned());
        fresh_artifact_of(rung, &opts, builders, renderers, &mut on_stage)
    };
    let mut path = built.map_err(|err| export_failed(err.message()))?;
    if let Some(out) = &options.out {
        path = resolve_out(&path, out)?;
    }
    Ok(result(path, format, state, available, stages))
}
