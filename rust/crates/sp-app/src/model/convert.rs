//! What the Convert surface decides (was
//! `desktop/ui/convert.js`, above its line).
//! Every function here is pure and is tested below the way
//! tests/desktop-ui.test.ts tested the JavaScript.

use serde_json::{Map, Value};

use super::json::{error_message, ok, str_at};
use super::names;

/// The heading names the CAUSE, not the failure.
pub const HEADINGS: &[(&str, &str)] = &[
    ("scanned", "Scanned PDF, no text"),
    ("not-screenplay", "Not a screenplay"),
    ("password", "Locked PDF"),
    ("unreadable", "Unreadable file"),
    ("unsupported-type", "Wrong kind of file"),
    ("bad-options", "Bad settings"),
    ("usage", "Bad settings"),
    ("library", "No way into the library"),
    (
        "unwritable",
        "Method Reader could not save this script's files",
    ),
    ("internal", "The engine did not answer"),
];

pub fn heading_for(code: &str) -> Option<&'static str> {
    HEADINGS.iter().find(|(c, _)| *c == code).map(|(_, h)| *h)
}

/// The window's own name, as its title bar shows it (the idle screen no
/// longer sets it as a wordmark: the title bar says what the window is).
pub const WORDMARK: &str = super::names::PRODUCT;

/// The window's title: the script's name before the app's, as Method
/// Budget's "Name - Method Budget", or the app's alone with no script.
pub fn window_title(script: Option<&str>) -> String {
    match script.map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => format!("{name} - {WORDMARK}"),
        None => WORDMARK.to_string(),
    }
}

/// The drop well's words.
pub const WELL_CALL: &str = "Drop a screenplay PDF";
pub const WELL_OR: &str = "or";
/// The well's one button: the page's primary action.
pub const CHOOSE: &str = "Choose PDF…";
pub const WELL_LIMITS: &str = "Needs selectable text, not a scan.";

/// The one guard a reader can meaningfully overrule.
pub const OVERRIDABLE: &str = "not-screenplay";
/// The flag the override button stands for (app.js FORCE_FLAG).
pub const FORCE_FLAG: &str = "--force";

pub const NO_MESSAGE: &str = "The engine refused the file without saying why.";

/// The bar's position and words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub percent: u8,
    pub stage: Option<Stage>,
    pub label: String,
}

impl Progress {
    /// The bar before the engine's first line arrives.
    pub fn start() -> Self {
        Self {
            percent: 0,
            stage: None,
            label: "starting up".into(),
        }
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::start()
    }
}

/// The stages the engine names, in the order it passes through them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Parse,
    Pictures,
    Render,
}

impl Stage {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "parse" => Some(Self::Parse),
            "pictures" => Some(Self::Pictures),
            "render" => Some(Self::Render),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Parse => "reading the pages",
            Self::Pictures => "reading the pictures",
            Self::Render => "building the book",
        }
    }
}

/// The bar's next position, given one progress line (the `progress` object
/// the engine writes). The percent is already the whole job's; it is taken
/// as given, clamped, and never allowed to fall; the stage never walks back.
/// Returns None when the line changes nothing (the JS returned `previous`).
pub fn next_progress(previous: &Progress, line: &Value) -> Option<Progress> {
    let stage = Stage::from_name(str_at(line, "stage")?)?;
    let percent = line.get("percent")?.as_f64()?;
    let next = percent.round().clamp(0.0, 100.0) as u8;
    if next < previous.percent {
        return None;
    }
    if previous.stage.is_some_and(|was| stage < was) {
        return None;
    }
    if next == previous.percent && Some(stage) == previous.stage {
        return None;
    }
    Some(Progress {
        percent: next,
        stage: Some(stage),
        label: format!("{} ({next}%)", stage.label()),
    })
}

/// What the read-out says while a conversion waits its turn on its book, by
/// what holds it: the Settings page's words for the same wait, longest wait
/// first. None when nothing does.
pub fn waiting_label(ahead: &[&str]) -> Option<String> {
    if ahead.is_empty() {
        return None;
    }
    if let Some((_, words)) = super::tune::WAITING
        .iter()
        .find(|(label, _)| ahead.contains(label))
    {
        return Some((*words).to_string());
    }
    if ahead.contains(&"save") {
        return Some("Waiting for the settings to finish saving…".into());
    }
    Some("Waiting for the book to be free…".into())
}

/// Which of the three platforms the window runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
    Unknown,
}

impl Platform {
    /// The platform this binary was built for.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unknown
        }
    }

    /// The word the request service and the bug report use.
    pub fn word(self) -> &'static str {
        match self {
            Self::Mac => "mac",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Unknown => "unknown",
        }
    }
}

pub fn shortcut_label(platform: Platform) -> &'static str {
    if platform == Platform::Mac {
        "⌘O"
    } else {
        "Ctrl+O"
    }
}

/// What the file manager is called where this window runs.
pub fn reveal_label(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => "Show in Finder",
        Platform::Windows => "Show in File Explorer",
        _ => "Show in folder",
    }
}

/// Take out the sentence telling a CLI user to type the flag the window has
/// drawn a button for. Never down to nothing.
pub fn without_cli_remedy(message: &str, flag: &str) -> String {
    let whole = message.trim();
    if flag.is_empty() || !whole.contains(flag) {
        return whole.to_string();
    }
    let kept: String = sentences(whole)
        .into_iter()
        .filter(|s| !s.contains(flag))
        .collect();
    let kept = kept.trim();
    if kept.is_empty() {
        whole.to_string()
    } else {
        kept.to_string()
    }
}

/// `/[^.!?]+[.!?]*\s*/g`: each run up to and including its stops and the
/// space after them.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        while i < chars.len() && !matches!(chars[i], '.' | '!' | '?') {
            i += 1;
        }
        if i == start {
            // A stop with nothing before it is not matched by the pattern.
            i += 1;
            continue;
        }
        while i < chars.len() && matches!(chars[i], '.' | '!' | '?') {
            i += 1;
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        out.push(chars[start..i].iter().collect());
    }
    out
}

/// What a failing answer means for the reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: String,
    pub heading: &'static str,
    pub message: String,
    pub can_force: bool,
}

/// `failureFor(answer.error)`.
pub fn failure_for(error: Option<&Value>) -> Refusal {
    let empty = Value::Object(Map::new());
    let error = error.unwrap_or(&empty);
    let raw = str_at(error, "code").map(str::trim).unwrap_or("");
    let code = if raw.is_empty() { "internal" } else { raw };
    let message = str_at(error, "message").map(str::trim).unwrap_or("");
    let can_force = code == OVERRIDABLE;
    let said = if message.is_empty() {
        NO_MESSAGE
    } else {
        message
    };
    Refusal {
        code: code.to_string(),
        heading: heading_for(code).unwrap_or_else(|| heading_for("internal").unwrap_or("")),
        message: if can_force {
            without_cli_remedy(said, FORCE_FLAG)
        } else {
            said.to_string()
        },
        can_force,
    }
}

/// A refusal built from a call that could not run at all.
pub fn internal_failure(message: &str) -> Refusal {
    failure_for(Some(
        &serde_json::json!({"code": "internal", "message": message}),
    ))
}

/// What Show in Finder says when the engine could not reveal the file.
pub fn reveal_failure_message(answer: &Value) -> Option<String> {
    if ok(answer) {
        return None;
    }
    Some(failure_for(answer.get("error")).message)
}

/// What the refusal says about the book that was already open.
pub fn still_open_note(script: Option<&Script>) -> Option<String> {
    let title = script?.title.trim();
    if title.is_empty() {
        None
    } else {
        Some(format!(
            "{title} is still open. This changed nothing about it."
        ))
    }
}

/// The name at the end of a path, whichever slash the OS used.
pub fn file_name(path: &str) -> String {
    path.split(['/', '\\'])
        .rfind(|p| !p.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

/// "19 pages, 11 scenes", singulars included; a missing count left out.
pub fn counts_line(pages: Option<i64>, scenes: Option<i64>) -> String {
    let say = |n: i64, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    [
        pages.map(|n| say(n, "page")),
        scenes.map(|n| say(n, "scene")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ")
}

/// The result's heading.
pub fn ready_line(title: &str) -> String {
    let name = title.trim();
    format!(
        "{} is ready",
        if name.is_empty() { "The book" } else { name }
    )
}

/// The open script, every optional field normalised once.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub path: String,
    pub title: String,
    pub author: Option<String>,
    pub pages: Option<i64>,
    pub scenes: Option<i64>,
    pub warnings: Vec<String>,
    pub epub_path: Option<String>,
    pub fountain_path: Option<String>,
    /// The engine's own preview document (`previewHtml`, the EPUB's
    /// XHTML with css.ts inline); empty when it sent none (Read's blank
    /// notice).
    pub preview_html: String,
    /// The script's settings once this window has read them.
    pub settings: Option<Map<String, Value>>,
}

/// `scriptFrom(path, answer)`.
pub fn script_from(path: &str, answer: &Value) -> Script {
    let text = |key: &str| str_at(answer, key).map(str::to_string);
    Script {
        path: path.to_string(),
        title: text("title").unwrap_or_else(|| file_name(path)),
        author: text("author"),
        pages: answer.get("pages").and_then(Value::as_i64),
        scenes: answer.get("scenes").and_then(Value::as_i64),
        warnings: answer
            .get("warnings")
            .and_then(Value::as_array)
            .map(|w| {
                w.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        epub_path: text("epubPath"),
        fountain_path: text("fountainPath"),
        preview_html: text("previewHtml").unwrap_or_default(),
        settings: None,
    }
}

/// Which of the dropped paths this window converts.
pub fn dropped_path(paths: &[String]) -> Option<String> {
    paths.iter().find(|p| !p.trim().is_empty()).cloned()
}

/// Where books are saved, as `app-settings --json` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    pub path: String,
    pub chosen: Option<String>,
    pub platform_default: String,
    pub from_env: bool,
    pub home: String,
}

/// `libraryFrom(answer)`: None for anything the contract does not promise.
pub fn library_from(answer: &Value) -> Option<Library> {
    if !ok(answer) {
        return None;
    }
    let library = answer.get("library")?.as_object()?;
    let path = library.get("path")?.as_str()?;
    let chosen = match library.get("chosen") {
        Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        _ => return None,
    };
    let platform_default = library.get("platformDefault")?.as_str()?;
    let from_env = library.get("fromEnv")?.as_bool()?;
    let home = answer.get("home")?.as_str()?;
    Some(Library {
        path: path.into(),
        chosen,
        platform_default: platform_default.into(),
        from_env,
        home: home.into(),
    })
}

/// The library line itself, naming the variable this process was given
/// (METHOD_READER_LIBRARY, or the LEGACY SCREEPUB_LIBRARY when only that
/// one is set), which is the one the engine read.
pub fn library_line(library: &Library) -> String {
    let var = names::var_with_legacy(
        &names::process_env,
        names::LIBRARY_VAR,
        names::LEGACY_LIBRARY_VAR,
    )
    .map_or(names::LIBRARY_VAR, |(_, var)| var);
    library_line_for(library, var)
}

/// [`library_line`], with the variable named.
pub fn library_line_for(library: &Library, var: &str) -> String {
    let shown = shorten_home(&library.path, &library.home);
    if library.from_env {
        format!("Books are saved in {shown}, set by {var}.")
    } else {
        format!("Books are saved in {shown}.")
    }
}

/// `path` with a leading `home` folded to `~`, only when it genuinely sits
/// under home; case-insensitive for Windows paths.
pub fn shorten_home(path: &str, home: &str) -> String {
    let windows = path.contains('\\') || home.contains('\\') || drive_letter(path);
    let fold = |s: &str| {
        if windows {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let root = home.trim_end_matches(['/', '\\']);
    if fold(path) == fold(home) || fold(path) == fold(root) {
        return "~".into();
    }
    for sep in ['/', '\\'] {
        let prefix = format!("{root}{sep}");
        if fold(path).starts_with(&fold(&prefix)) {
            return format!("~{}", &path[root.len()..]);
        }
    }
    path.to_string()
}

fn drive_letter(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Which buttons belong beside the library line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryAction {
    Change,
    Reset,
}

pub fn library_actions(library: &Library) -> Vec<LibraryAction> {
    if library.from_env {
        vec![]
    } else if library.chosen.is_none() {
        vec![LibraryAction::Change]
    } else {
        vec![LibraryAction::Change, LibraryAction::Reset]
    }
}

pub const MOVED_LINE: &str = "New books go here. Books already converted stay where they are.";
pub const NO_LIBRARY_MESSAGE: &str = "Method Reader could not confirm where books are saved.";

/// The value `app-settings --set` takes for Change (a folder) or Reset.
pub fn library_change_args(path: Option<&str>) -> String {
    serde_json::json!({ "libraryPath": path }).to_string()
}

/// What Change or Reset shows once the engine has answered.
pub fn library_after(answer: &Value) -> Result<Library, String> {
    library_from(answer)
        .ok_or_else(|| error_message(answer).unwrap_or_else(|| NO_LIBRARY_MESSAGE.to_string()))
}

/// Only a Change that actually moved the library says MOVED_LINE.
pub fn shows_moved_line(action: LibraryAction, previous_path: &str, library: &Library) -> bool {
    action == LibraryAction::Change && library.path != previous_path
}

/// A line of text from the engine's diagnostic stream, decoded: each line
/// that is a JSON object with a `progress` member gives that member.
pub fn progress_lines(payload: &str) -> Vec<Value> {
    payload
        .split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| v.get("progress").cloned())
        .filter(|p| !p.is_null() && p != &Value::Bool(false))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_shortcut_is_spelled_the_platforms_way() {
        assert_eq!(shortcut_label(Platform::Mac), "⌘O");
        assert_eq!(shortcut_label(Platform::Linux), "Ctrl+O");
        assert_eq!(shortcut_label(Platform::Windows), "Ctrl+O");
        assert_eq!(shortcut_label(Platform::Unknown), "Ctrl+O");
    }

    #[test]
    fn the_file_manager_is_named_for_the_platform() {
        assert_eq!(reveal_label(Platform::Mac), "Show in Finder");
        assert_eq!(reveal_label(Platform::Windows), "Show in File Explorer");
        assert_eq!(reveal_label(Platform::Linux), "Show in folder");
        assert_eq!(reveal_label(Platform::Unknown), "Show in folder");
    }

    #[test]
    fn every_code_the_engine_can_refuse_with_gets_a_heading_that_names_a_cause() {
        let codes = [
            "scanned",
            "not-screenplay",
            "password",
            "unreadable",
            "unsupported-type",
            "bad-options",
            "usage",
            "library",
            "unwritable",
            "internal",
        ];
        let mut headings = std::collections::BTreeSet::new();
        for code in codes {
            assert!(heading_for(code).is_some(), "{code} has its own heading");
            headings.insert(failure_for(Some(&json!({"code": code, "message": "x"}))).heading);
        }
        assert!(headings.len() >= codes.len() - 1);
        let unknown = failure_for(Some(&json!({"code": "moon-phase", "message": "x"})));
        assert_eq!(unknown.heading, "The engine did not answer");
        assert!(!unknown.can_force);
        assert_eq!(unknown.code, "moon-phase");
    }

    #[test]
    fn the_engines_sentence_survives_verbatim() {
        for (code, said) in [
            ("unreadable", "cannot read the input file (EACCES)"),
            ("password", "This PDF is password-protected."),
            (
                "scanned",
                "This PDF is a scan. Pass --force to convert it anyway.",
            ),
        ] {
            let shown = failure_for(Some(&json!({"code": code, "message": said})));
            assert_eq!(shown.message, said);
        }
    }

    #[test]
    fn the_refusal_stops_telling_a_window_user_to_type_the_flag_it_drew_a_button_for() {
        let said = "No scene headings and no dialogue found: this does not look like a \
                    screenplay. Pass --force to convert it anyway.";
        let shown = failure_for(Some(&json!({"code": "not-screenplay", "message": said})));
        assert_eq!(
            shown.message,
            "No scene headings and no dialogue found: this does not look like a screenplay."
        );
        assert!(shown.can_force);
    }

    #[test]
    fn a_remedy_that_is_the_whole_message_is_kept() {
        assert_eq!(
            without_cli_remedy("Pass --force to convert it anyway.", "--force"),
            "Pass --force to convert it anyway."
        );
        assert_eq!(without_cli_remedy("  plain  ", ""), "plain");
    }

    #[test]
    fn a_failure_with_no_sentence_still_says_something() {
        for broken in [
            Some(json!({"code": "scanned"})),
            Some(json!({"code": "scanned", "message": "   "})),
            Some(json!({})),
            None,
        ] {
            let shown = failure_for(broken.as_ref());
            assert_eq!(shown.message, NO_MESSAGE);
            assert!(!shown.heading.is_empty());
        }
        assert_eq!(failure_for(Some(&json!({}))).code, "internal");
        assert!(!failure_for(Some(&json!({}))).can_force);
    }

    #[test]
    fn only_not_screenplay_may_be_overruled() {
        assert!(failure_for(Some(&json!({"code": "not-screenplay", "message": "x"}))).can_force);
        for code in ["scanned", "password", "unreadable", "usage", "internal"] {
            assert!(!failure_for(Some(&json!({"code": code, "message": "x"}))).can_force);
        }
    }

    fn walk(lines: &[Value]) -> Vec<Progress> {
        let mut at = Progress::start();
        lines
            .iter()
            .map(|l| {
                if let Some(next) = next_progress(&at, l) {
                    at = next;
                }
                at.clone()
            })
            .collect()
    }

    #[test]
    fn the_bar_follows_the_engines_own_percent() {
        let seen: Vec<u8> = walk(&[
            json!({"stage": "parse", "percent": 6}),
            json!({"stage": "parse", "percent": 44}),
            json!({"stage": "parse", "percent": 60}),
            json!({"stage": "pictures", "percent": 60}),
            json!({"stage": "pictures", "percent": 73}),
            json!({"stage": "pictures", "percent": 85}),
            json!({"stage": "render", "percent": 85}),
            json!({"stage": "render", "percent": 100}),
        ])
        .iter()
        .map(|p| p.percent)
        .collect();
        assert_eq!(seen, vec![6, 44, 60, 60, 73, 85, 85, 100]);
    }

    #[test]
    fn the_read_out_names_the_stage() {
        assert_eq!(Progress::start().label, "starting up");
        let parsed =
            next_progress(&Progress::start(), &json!({"stage":"parse","percent":60})).unwrap();
        assert_eq!(parsed.label, "reading the pages (60%)");
        let pictures = next_progress(&parsed, &json!({"stage":"pictures","percent":60})).unwrap();
        assert_eq!(pictures.label, "reading the pictures (60%)");
        let render = next_progress(&parsed, &json!({"stage":"render","percent":85})).unwrap();
        assert_eq!(render.label, "building the book (85%)");
    }

    #[test]
    fn a_line_that_is_late_unknown_or_malformed_leaves_the_bar_alone() {
        let half =
            next_progress(&Progress::start(), &json!({"stage":"render","percent":50})).unwrap();
        for junk in [
            json!({"stage":"parse","percent":20}),
            json!({"stage":"pictures","percent":40}),
            json!({"stage":"render","percent":50}),
            json!({"stage":"polish","percent":99}),
            json!({"stage":"render","percent":"lots"}),
            json!({"stage":"render"}),
            json!({}),
            Value::Null,
        ] {
            assert_eq!(next_progress(&half, &junk), None, "{junk}");
        }
        assert_eq!(
            next_progress(&half, &json!({"stage":"render","percent":900}))
                .unwrap()
                .percent,
            100
        );
        let parsed =
            next_progress(&Progress::start(), &json!({"stage":"parse","percent":60})).unwrap();
        let pictures = next_progress(&parsed, &json!({"stage":"pictures","percent":60})).unwrap();
        assert_eq!(
            next_progress(&pictures, &json!({"stage":"parse","percent":60})),
            None
        );
    }

    #[test]
    fn the_counts_are_one_line_singular_included() {
        assert_eq!(counts_line(Some(19), Some(11)), "19 pages, 11 scenes");
        assert_eq!(counts_line(Some(1), Some(1)), "1 page, 1 scene");
        assert_eq!(counts_line(Some(1), None), "1 page");
        assert_eq!(counts_line(None, None), "");
    }

    #[test]
    fn the_ready_line_names_the_book() {
        assert_eq!(ready_line("Field Station"), "Field Station is ready");
        assert_eq!(ready_line("  "), "The book is ready");
    }

    #[test]
    fn a_file_name_is_the_last_segment_whichever_slash() {
        assert_eq!(file_name("/a/b/c.pdf"), "c.pdf");
        assert_eq!(file_name("C:\\a\\c.pdf"), "c.pdf");
        assert_eq!(file_name("c.pdf"), "c.pdf");
        assert_eq!(file_name("/a/b/"), "b");
    }

    #[test]
    fn the_still_open_note_names_the_book_or_nothing() {
        let mut script = script_from("/x/a.pdf", &json!({"title": "Field Station"}));
        assert_eq!(
            still_open_note(Some(&script)).as_deref(),
            Some("Field Station is still open. This changed nothing about it.")
        );
        script.title = " ".into();
        assert_eq!(still_open_note(Some(&script)), None);
        assert_eq!(still_open_note(None), None);
    }

    #[test]
    fn a_script_is_normalised_once() {
        let s = script_from("/x/Dropped.pdf", &json!({"ok": true, "warnings": ["w", 3]}));
        assert_eq!(s.title, "Dropped.pdf");
        assert_eq!(s.warnings, vec!["w".to_string()]);
        assert_eq!(s.epub_path, None);
        assert!(s.preview_html.is_empty());
    }

    #[test]
    fn the_first_dropped_path_with_a_name_converts() {
        assert_eq!(dropped_path(&[]), None);
        assert_eq!(
            dropped_path(&["  ".into(), "/a.pdf".into(), "/b.pdf".into()]),
            Some("/a.pdf".into())
        );
    }

    fn lib(path: &str, chosen: Option<&str>, from_env: bool, home: &str) -> Value {
        json!({"ok": true, "home": home, "library": {
            "path": path, "chosen": chosen, "platformDefault": "/d", "fromEnv": from_env}})
    }

    #[test]
    fn the_library_line_folds_home_only_when_it_is_really_under_it() {
        let l = library_from(&lib("/Users/ann/Method Reader", None, false, "/Users/ann")).unwrap();
        assert_eq!(library_line(&l), "Books are saved in ~/Method Reader.");
        assert_eq!(shorten_home("/Users/ann2/x", "/Users/ann"), "/Users/ann2/x");
        assert_eq!(shorten_home("/Users/ann", "/Users/ann/"), "~");
        assert_eq!(shorten_home("/", "/"), "~");
        assert_eq!(shorten_home("/x", "/"), "~/x");
        assert_eq!(shorten_home("c:\\users\\ann\\b", "C:\\Users\\Ann"), "~\\b");
        assert_eq!(shorten_home("/Users/Ann/b", "/Users/ann"), "/Users/Ann/b");
        let env = library_from(&lib("/srv/books", None, true, "/Users/ann")).unwrap();
        assert_eq!(
            library_line_for(&env, names::LIBRARY_VAR),
            "Books are saved in /srv/books, set by METHOD_READER_LIBRARY."
        );
    }

    #[test]
    fn the_library_answer_is_taken_only_whole() {
        assert!(library_from(&json!({"ok": false})).is_none());
        assert!(library_from(&json!({"ok": true, "library": {"path": 1}})).is_none());
        let mut bad = lib("/a", None, false, "/h");
        bad["library"]["chosen"] = json!(3);
        assert!(library_from(&bad).is_none());
        let mut no_home = lib("/a", None, false, "/h");
        no_home.as_object_mut().unwrap().remove("home");
        assert!(library_from(&no_home).is_none());
    }

    #[test]
    fn the_library_buttons_follow_what_was_chosen() {
        let default = library_from(&lib("/a", None, false, "/h")).unwrap();
        let chosen = library_from(&lib("/a", Some("/a"), false, "/h")).unwrap();
        let env = library_from(&lib("/a", Some("/a"), true, "/h")).unwrap();
        assert_eq!(library_actions(&default), vec![LibraryAction::Change]);
        assert_eq!(
            library_actions(&chosen),
            vec![LibraryAction::Change, LibraryAction::Reset]
        );
        assert!(library_actions(&env).is_empty());
    }

    #[test]
    fn change_and_reset_answer_with_a_library_or_a_sentence() {
        assert_eq!(
            library_change_args(Some("/abs")),
            r#"{"libraryPath":"/abs"}"#
        );
        assert_eq!(library_change_args(None), r#"{"libraryPath":null}"#);
        assert!(library_after(&lib("/a", None, false, "/h")).is_ok());
        assert_eq!(
            library_after(&json!({"ok": false, "error": {"message": "no"}})),
            Err("no".into())
        );
        assert_eq!(
            library_after(&json!({"ok": true})),
            Err(NO_LIBRARY_MESSAGE.into())
        );
        let moved = library_from(&lib("/b", Some("/b"), false, "/h")).unwrap();
        assert!(shows_moved_line(LibraryAction::Change, "/a", &moved));
        assert!(!shows_moved_line(LibraryAction::Change, "/b", &moved));
        assert!(!shows_moved_line(LibraryAction::Reset, "/a", &moved));
    }

    #[test]
    fn a_reveal_refusal_reads_like_every_other() {
        assert_eq!(reveal_failure_message(&json!({"ok": true})), None);
        assert_eq!(
            reveal_failure_message(
                &json!({"ok": false, "error": {"code": "x", "message": "gone"}})
            ),
            Some("gone".into())
        );
        assert_eq!(
            reveal_failure_message(&json!({"ok": false})),
            Some(NO_MESSAGE.into())
        );
    }

    #[test]
    fn a_conversion_waits_in_the_settings_pages_words() {
        assert_eq!(waiting_label(&[]), None);
        assert_eq!(
            waiting_label(&["send", "kindle"]).as_deref(),
            Some("Waiting for the Kindle file to finish building…")
        );
        assert_eq!(
            waiting_label(&["save"]).as_deref(),
            Some("Waiting for the settings to finish saving…")
        );
        assert_eq!(
            waiting_label(&["check"]).as_deref(),
            Some("Waiting for the book to be free…")
        );
    }

    #[test]
    fn progress_lines_are_read_off_the_diagnostic_stream() {
        let lines = progress_lines(
            "{\"progress\":{\"stage\":\"parse\",\"percent\":5}}\nwarning: x\n\n{\"other\":1}\n",
        );
        assert_eq!(lines, vec![json!({"stage":"parse","percent":5})]);
    }
}
