//! Port of `src/export/kfx.ts`: the KFX rung. The only KFX writer in
//! existence is inside Amazon's Kindle Previewer, so the rung needs Calibre,
//! Kindle Previewer and jhowell's KFX Output plugin. Amazon ships Previewer
//! for macOS and Windows only: on Linux the rung is never ready and the
//! ladder takes the AZW3.
//!
//! Also the plugin install, which runs inside Calibre's own Python
//! (`calibre-debug -c`), exactly as the TypeScript does: Calibre's
//! cert-pinned fetch for the index, a size check on the plugin zip, a
//! "this is a calibre plugin" check, and Calibre's own `add_plugin`. No
//! HTTP happens in Rust at all.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde_json::Value;

use crate::env::{Env, host_platform};
use crate::errors::EngineError;
use crate::export::calibre::{
    CALIBRE_FORMAT_GUARDS, CONVERSION_TIMEOUT_MS, TimedRun, calibre_failed, calibre_missing,
    calibre_tool, run_calibre, run_with_timeout, strip_epub,
};
use crate::fsx;
use crate::nodepath::host;

/// How long `calibre-customize --list-plugins` may take before the plugin
/// reads as not installed.
pub const PLUGIN_LIST_TIMEOUT_MS: u64 = 60 * 1000;

/// How long the plugin install may take: two downloads and `add_plugin`.
pub const PLUGIN_INSTALL_TIMEOUT_MS: u64 = 5 * 60 * 1000;

/// `KfxStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KfxStatus {
    pub calibre: bool,
    pub previewer: bool,
    /// Only meaningful when `calibre` is true: the plugin lives inside it.
    pub plugin_installed: bool,
    pub ready: bool,
}

/// `previewerPath()`.
pub fn previewer_path(env: &Env) -> Option<String> {
    match host_platform() {
        "darwin" => {
            let app = "/Applications/Kindle Previewer 3.app";
            fsx::exists(app).then(|| app.to_owned())
        }
        "win32" => {
            let local = env.non_empty("LOCALAPPDATA")?;
            let exe = host::join(&[
                local,
                "Amazon",
                "Kindle Previewer 3",
                "Kindle Previewer 3.exe",
            ]);
            fsx::exists(&exe).then_some(exe)
        }
        _ => None,
    }
}

/// `pluginInstalled(customize, timeoutMs)`: whether this Calibre has the
/// KFX Output plugin. A listing that does not finish in time, or fails,
/// reads as not installed.
pub fn plugin_installed(customize: &str, env: &Env, timeout_ms: u64) -> bool {
    match run_with_timeout(
        &[customize.to_owned(), "--list-plugins".to_owned()],
        env,
        timeout_ms,
    ) {
        Ok(TimedRun {
            code: 0,
            stdout,
            timed_out: false,
            ..
        }) => lists_kfx_output(&stdout),
        _ => false,
    }
}

/// `split(/\r?\n/)`.
fn lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

/// `pluginTableHeader(listing)`: the `Type Name Version` header row,
/// wherever it is, or `None`.
pub fn plugin_table_header(listing: &str) -> Option<&str> {
    lines(listing).into_iter().find(|line| is_header(line))
}

/// `/^Type +Name +Version\b/`.
fn is_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("Type") else {
        return false;
    };
    let rest_trimmed = rest.trim_start_matches(' ');
    if rest_trimmed.len() == rest.len() {
        return false;
    }
    let Some(rest) = rest_trimmed.strip_prefix("Name") else {
        return false;
    };
    let after = rest.trim_start_matches(' ');
    if after.len() == rest.len() {
        return false;
    }
    let Some(tail) = after.strip_prefix("Version") else {
        return false;
    };
    // `\b`: the next character is not a word character.
    !tail
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `listsKfxOutput(listing)`: a plugin NAMED exactly "KFX Output", or a
/// renamed copy named "KFX Output (" and something, read off the table's
/// own columns. The companion "Set KFX metadata (from KFX Output)" does not
/// count; the Disabled column is not consulted.
pub fn lists_kfx_output(listing: &str) -> bool {
    let Some(header) = plugin_table_header(listing) else {
        return false;
    };
    // The header is ASCII, so its byte offsets are its character offsets.
    let name_at = header.find("Name").unwrap_or(0);
    let version_at = header.find("Version").unwrap_or(0);
    lines(listing).into_iter().any(|line| {
        if line == header || line.chars().next().is_none_or(char::is_whitespace) {
            return false;
        }
        // `line.slice(nameAt, versionAt)`, in UTF-16 code units.
        let units: Vec<u16> = line.encode_utf16().collect();
        let start = name_at.min(units.len());
        let end = version_at.min(units.len()).max(start);
        let name = String::from_utf16_lossy(&units[start..end]);
        let name = crate::env::js_trim(&name);
        name == "KFX Output" || name.starts_with("KFX Output (")
    })
}

/// `computeReady(s)`: all three tools.
pub fn compute_ready(calibre: bool, previewer: bool, plugin_installed: bool) -> bool {
    calibre && previewer && plugin_installed
}

/// `kfxStatus()`: what is present on this machine. Spawns
/// `calibre-customize` (about a second of Python) only when Calibre is
/// there, with `env` (which must carry a scratch CALIBRE_CONFIG_DIRECTORY
/// in any test).
pub fn kfx_status(env: &Env) -> KfxStatus {
    let customize = calibre_tool("calibre-customize", env);
    let calibre = customize.is_some();
    let previewer = previewer_path(env).is_some();
    let installed = customize.is_some_and(|c| plugin_installed(&c, env, PLUGIN_LIST_TIMEOUT_MS));
    KfxStatus {
        calibre,
        previewer,
        plugin_installed: installed,
        ready: compute_ready(calibre, previewer, installed),
    }
}

/// `kfxSibling(epub)`: same folder, same stem, `.kfx`.
pub fn kfx_sibling(epub: &str) -> String {
    format!("{}.kfx", strip_epub(epub))
}

/// `kfxScratchPath(epub)`: `.<stem>.partial.kfx` in the same folder (same
/// volume, the `.kfx` extension ebook-convert needs, hidden).
pub fn kfx_scratch_path(epub: &str) -> String {
    let base = host::basename(epub);
    let stem = strip_epub(&base);
    host::join(&[host::dirname(epub), format!(".{stem}.partial.kfx")])
}

/// `kfxMissing(status)`: what a KFX build still needs, named for a person.
pub fn kfx_missing(status: &KfxStatus) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !status.calibre {
        missing.push("Calibre");
    }
    if !status.previewer {
        missing.push("Kindle Previewer");
    }
    if status.calibre && !status.plugin_installed {
        missing.push("the KFX plugin");
    }
    missing
}

/// `KfxToolchainNotReadyError(status)`.
pub fn kfx_not_ready(status: &KfxStatus) -> EngineError {
    EngineError::Named {
        name: "KfxToolchainNotReadyError",
        message: format!(
            "KFX conversion needs {}.",
            kfx_missing(status).join(" and ")
        ),
    }
}

/// `KfxDeps`: seams for `to_kfx`, each defaulting to the real thing.
#[derive(Default)]
pub struct KfxDeps<'a> {
    pub tool: Option<Box<dyn Fn() -> Option<String> + 'a>>,
    pub status: Option<Box<dyn Fn() -> KfxStatus + 'a>>,
    pub timeout_ms: Option<u64>,
}

/// `mkdtempSync(join(tmpdir(), prefix))`.
pub(crate) fn make_temp_dir(prefix: &str) -> Result<String, EngineError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir();
    let base = base.to_string_lossy();
    loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let tag = format!(
            "{:06x}",
            (u64::from(nanos)
                ^ (u64::from(std::process::id()) << 20)
                ^ n.wrapping_mul(0x9e37_79b9))
                & 0xff_ffff
        );
        let dir = host::join(&[base.as_ref(), &format!("{prefix}{tag}")]);
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(EngineError::Fs(crate::errors::FsError::from_io(
                    &e, "mkdtemp", &dir, None,
                )));
            }
        }
    }
}

/// `toKfx(epub, onStage, deps)`: EPUB to KFX through ebook-convert, with
/// the format guards, into a hidden scratch file renamed into place, and
/// with Kindle Previewer given a temp folder of its own (TMPDIR, TMP and
/// TEMP) that is removed afterwards. `env` is the environment every spawn
/// gets.
pub fn to_kfx(
    epub: &str,
    on_stage: &mut dyn FnMut(&str),
    deps: &KfxDeps,
    env: &Env,
) -> Result<String, EngineError> {
    let tool = match &deps.tool {
        Some(f) => f(),
        None => calibre_tool("ebook-convert", env),
    }
    .ok_or_else(calibre_missing)?;
    let status = match &deps.status {
        Some(f) => f(),
        None => kfx_status(env),
    };
    if !status.ready {
        return Err(kfx_not_ready(&status));
    }
    let kfx = kfx_sibling(epub);
    let scratch = kfx_scratch_path(epub);
    let previewer_tmp = make_temp_dir("method-reader-kfx-")?;
    let child_env = env
        .with("TMPDIR", &previewer_tmp)
        .with("TMP", &previewer_tmp)
        .with("TEMP", &previewer_tmp);
    on_stage("converting to KFX (Kindle Previewer can take ~20s to start)…");
    let mut args = vec![epub.to_owned(), scratch.clone()];
    args.extend(CALIBRE_FORMAT_GUARDS.iter().map(|s| (*s).to_owned()));
    let result = run_calibre(
        &tool,
        &args,
        &child_env,
        deps.timeout_ms.unwrap_or(CONVERSION_TIMEOUT_MS),
    )
    .and_then(|()| {
        if !fsx::exists(&scratch) {
            return Err(calibre_failed(
                "ebook-convert exited cleanly but produced no .kfx",
            ));
        }
        fsx::rename(&scratch, &kfx)
    });
    if result.is_err() {
        fsx::rm_force(&scratch);
    }
    fsx::rm_tree_force(&previewer_tmp);
    result.map(|()| kfx)
}

/// The Python that installs the plugin inside Calibre, byte for byte the
/// TypeScript's `INSTALL_SNIPPET`.
pub const INSTALL_SNIPPET: &str = include_str!("kfx_install.py");

/// `KfxInstallResult`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KfxInstallResult {
    pub ok: bool,
    /// The version Calibre installed, when it succeeded.
    pub version: Option<String>,
    /// Why not, in words a user can act on.
    pub reason: Option<String>,
    /// Conflicting KFX forks cleared to make room. `None` when Calibre's
    /// answer was not read (a timeout, no answer line).
    pub removed: Option<Vec<String>>,
}

/// The runner the install goes through: argv in, a finished run out.
pub type DebugRunner<'a> = dyn Fn(&[String]) -> Result<TimedRun, EngineError> + 'a;

/// `debugRunner(timeoutMs)`: the real runner, with `env`.
pub fn debug_runner(
    env: Env,
    timeout_ms: u64,
) -> impl Fn(&[String]) -> Result<TimedRun, EngineError> {
    move |argv: &[String]| run_with_timeout(argv, &env, timeout_ms)
}

fn truncate_units(text: &str, n: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

/// `installKfxPlugin(run, debugTool)`: install (or update to) the current
/// KFX Output plugin through Calibre's own index and installer. Never
/// fails: every outcome is a result.
pub fn install_kfx_plugin(run: &DebugRunner, debug_tool: Option<&str>) -> KfxInstallResult {
    let Some(debug_tool) = debug_tool else {
        return KfxInstallResult {
            ok: false,
            reason: Some("Calibre was not found, and the plugin lives inside it.".into()),
            ..Default::default()
        };
    };
    let argv = [
        debug_tool.to_owned(),
        "-c".to_owned(),
        INSTALL_SNIPPET.to_owned(),
    ];
    let run = match run(&argv) {
        Ok(run) => run,
        Err(err) => {
            return KfxInstallResult {
                ok: false,
                reason: Some(truncate_units(&err.message(), 400)),
                ..Default::default()
            };
        }
    };
    if run.timed_out {
        return KfxInstallResult {
            ok: false,
            reason: Some("Calibre was still installing the plugin after a long wait, so it was stopped. Check the internet connection, then try again.".into()),
            ..Default::default()
        };
    }
    // The result LINE decides, not the exit code.
    let Some(line) = run
        .stdout
        .split('\n')
        .find(|l| l.starts_with("METHOD_READER_RESULT "))
    else {
        let stderr = crate::env::js_trim(&run.stderr);
        let stdout = crate::env::js_trim(&run.stdout);
        let detail = if !stderr.is_empty() {
            stderr.to_owned()
        } else if !stdout.is_empty() {
            stdout.to_owned()
        } else {
            format!("calibre-debug exited {}", run.code)
        };
        return KfxInstallResult {
            ok: false,
            reason: Some(truncate_units(&detail, 400)),
            ..Default::default()
        };
    };
    let unreadable = || KfxInstallResult {
        ok: false,
        reason: Some(format!(
            "could not read calibre's answer: {}",
            truncate_units(line, 200)
        )),
        ..Default::default()
    };
    let parsed = match crate::jsjson::parse(&line["METHOD_READER_RESULT ".len()..]) {
        None | Some(Value::Null) => return unreadable(),
        Some(v) => v,
    };
    let field = |key: &str| parsed.as_object().and_then(|m| m.get(key));
    let truthy = |v: Option<&Value>| match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    };
    let removed: Vec<String> = match field("removed") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => crate::jsjson::stringify(other),
            })
            .collect(),
        _ => Vec::new(),
    };
    if truthy(field("ok")) && truthy(field("version")) {
        let version = match field("version") {
            Some(Value::String(s)) => s.clone(),
            Some(other) => crate::jsjson::stringify(other),
            None => String::new(),
        };
        return KfxInstallResult {
            ok: true,
            version: Some(version),
            reason: None,
            removed: Some(removed),
        };
    }
    if truthy(field("offline")) {
        return KfxInstallResult {
            ok: false,
            version: None,
            reason: Some("Calibre's plugin site could not be reached. Check the internet connection, then try again.".into()),
            removed: Some(removed),
        };
    }
    let reason = match field("error") {
        None | Some(Value::Null) => "calibre reported a failure with no reason".to_owned(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => crate::jsjson::stringify(other),
    };
    KfxInstallResult {
        ok: false,
        version: None,
        reason: Some(reason),
        removed: Some(removed),
    }
}
