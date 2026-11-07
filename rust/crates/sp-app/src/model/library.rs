//! The Library tab (docs/superpowers/specs/2026-11-02-library-view-design.md),
//! as the window decides it: the engine's `library` answer read field by
//! field, the words each row says, the empty library's words, how a
//! row's Open is asked and answered, and the question and answer of its
//! Remove from library (docs/superpowers/specs/2026-11-03-library-remove-design.md),
//! and its Settings (docs/superpowers/specs/2026-11-03-library-row-settings-design.md),
//! and its Rename (docs/superpowers/specs/2026-11-03-library-rename-design.md),
//! and its Report (docs/superpowers/specs/2026-11-04-conversion-report-design.md).
//! Every word about the Trash takes the platform: Windows calls it the
//! Recycle Bin ([`bin_name`]). No toolkit in it; the drawing is
//! src/app/library.rs.

use std::path::Path;

use serde_json::Value;

use super::json::{error_message, ok, text_at};

/// The page's title.
pub const TITLE: &str = "Your library";
pub const OPEN: &str = "Open";
pub const LOOKING: &str = "Looking in the library…";
pub const UNREADABLE: &str = "The library could not be read.";
/// The empty library's first line; [`empty_line`] is its second.
pub const EMPTY: &str = "Nothing in the library yet.";
/// A row's second button, which asks first.
pub const REMOVE: &str = "Remove from library";
/// A row's third button: Open, landing on the Settings tab.
pub const SETTINGS: &str = "Settings";
pub const CANCEL: &str = "Cancel";
/// A row's fourth button: a one-line field for the name it goes by.
pub const RENAME: &str = "Rename";
/// The rename field's first answer.
pub const SAVE: &str = "Save";
/// The longest name the field takes, in characters: the engine's own
/// limit (sp-engine's `library::MAX_NAME_CHARS`; tests/argv_contract.rs
/// holds the two together).
pub const MAX_NAME: usize = 200;

/// What the system Trash is called: the Recycle Bin on Windows, the Trash
/// everywhere else. Every Trash word below takes it. The command line's
/// own is sp-engine's `library_remove::bin_name`; tests/argv_contract.rs
/// holds the two to the same answer for both platforms.
pub fn bin_name(windows: bool) -> &'static str {
    if windows { "Recycle Bin" } else { "Trash" }
}

/// The question's first answer: "Move to Trash", or "Move to Recycle Bin"
/// on Windows.
pub fn move_button(windows: bool) -> String {
    format!("Move to {}", bin_name(windows))
}

/// What the question says goes, and what stays.
pub fn question_note(windows: bool) -> String {
    format!(
        "Its folder, book and settings go to the {}, where you can put them back. The PDF stays where it is.",
        bin_name(windows)
    )
}

/// What a removal says when its answer says nothing usable.
pub fn not_removed(windows: bool) -> String {
    format!(
        "The script was not moved to the {}, and nothing said why.",
        bin_name(windows)
    )
}

/// One script in the library, as the engine listed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub folder: String,
    /// The stem its files are named by.
    pub name: String,
    /// The `.fountain` title page's Title, when it has one.
    pub title: Option<String>,
    /// The file it was converted from.
    pub source: String,
    pub source_exists: bool,
    pub fountain: String,
    /// Its book, when the folder has one yet.
    pub epub: Option<String>,
    /// The highest page marker in its `.fountain`, when it has any.
    pub pages: Option<i64>,
    pub scenes: i64,
    /// When it was converted, RFC 3339 UTC.
    pub converted: String,
    /// Its own settings file, when it has one.
    pub settings: Option<String>,
}

/// `library`'s answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The library folder it read.
    pub root: String,
    pub rows: Vec<Row>,
}

/// A field that may be absent or null, but is of its kind when it is there:
/// Some(None) for absent or null, None for the wrong kind.
fn optional<T>(v: &Value, key: &str, read: impl Fn(&Value) -> Option<T>) -> Option<Option<T>> {
    match v.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(x) => read(x).map(Some),
    }
}

fn row_from(v: &Value) -> Option<Row> {
    let words = |x: &Value| {
        x.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Some(Row {
        folder: text_at(v, "folder")?,
        name: text_at(v, "name")?,
        title: optional(v, "title", |x| x.as_str().map(|s| s.trim().to_string()))?
            .filter(|s| !s.is_empty()),
        source: text_at(v, "source")?,
        source_exists: v.get("sourceExists")?.as_bool()?,
        fountain: text_at(v, "fountain")?,
        epub: optional(v, "epub", words)?,
        pages: optional(v, "pages", Value::as_i64)?,
        scenes: v.get("scenes")?.as_i64()?,
        converted: text_at(v, "converted")?,
        settings: optional(v, "settings", words)?,
    })
}

/// The answer, when it is one; a row with a broken field breaks the list,
/// as every list the window reads does.
pub fn listing_from(answer: &Value) -> Option<Listing> {
    if !ok(answer) {
        return None;
    }
    Some(Listing {
        root: text_at(answer, "root")?,
        rows: answer
            .get("scripts")?
            .as_array()?
            .iter()
            .map(row_from)
            .collect::<Option<_>>()?,
    })
}

/// Why there is no list: the engine's own sentence, or ours.
pub fn listing_failure(answer: &Value) -> String {
    error_message(answer).unwrap_or_else(|| UNREADABLE.to_string())
}

/// What a row is called: its title, or its name.
pub fn row_title(row: &Row) -> &str {
    row.title.as_deref().unwrap_or(&row.name)
}

fn count(n: i64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The day of an RFC 3339 time in words, `2 November 2026`; None for
/// anything that does not start with a date.
pub fn date_words(rfc3339: &str) -> Option<String> {
    let date = rfc3339.get(..10)?;
    let mut parts = date.split('-');
    let year: u32 = parts.next()?.parse().ok()?;
    let month: usize = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let month = MONTHS.get(month.checked_sub(1)?)?;
    (1..=31)
        .contains(&day)
        .then(|| format!("{day} {month} {year}"))
}

/// The line under a row's title: its pages (when its `.fountain` marks
/// them), its scenes, the day it was converted, and whether it has
/// settings of its own.
pub fn detail(row: &Row) -> String {
    let mut parts = Vec::new();
    if let Some(pages) = row.pages {
        parts.push(count(pages, "page", "pages"));
    }
    parts.push(count(row.scenes, "scene", "scenes"));
    if let Some(day) = date_words(&row.converted) {
        parts.push(format!("converted {day}"));
    }
    if row.settings.is_some() {
        parts.push("its own settings".to_string());
    }
    parts.join(", ")
}

/// What a row says when the file it came from has gone.
pub fn gone_note(row: &Row) -> Option<&'static str> {
    if row.source_exists {
        return None;
    }
    Some(if row.source.to_lowercase().ends_with(".pdf") {
        "Its PDF has moved or gone; the book still opens."
    } else {
        "The file it came from has moved or gone; the book still opens."
    })
}

/// The Open button's name for a screen reader.
pub fn open_label(row: &Row) -> String {
    format!("{OPEN} {}", row_title(row))
}

/// The Remove from library button's name for a screen reader.
pub fn remove_label(row: &Row) -> String {
    format!("Remove {} from the library", row_title(row))
}

/// The Settings button's name for a screen reader.
pub fn settings_label(row: &Row) -> String {
    format!("Settings for {}", row_title(row))
}

/// The Rename button's name for a screen reader.
pub fn rename_label(row: &Row) -> String {
    format!("Rename {}", row_title(row))
}

/// The rename field's name for a screen reader.
pub fn name_field_label(row: &Row) -> String {
    format!("New name for {}", row_title(row))
}

/// The rename field's Save button's name for a screen reader.
pub fn save_label(row: &Row) -> String {
    format!("Save the new name for {}", row_title(row))
}

/// What the tab says while a rename is under way.
pub fn renaming(row: &Row) -> String {
    format!("Renaming {}…", row_title(row))
}

/// What a rename says when its answer says nothing usable.
pub const NOT_RENAMED: &str = "The script was not renamed, and nothing said why.";

/// What `library-rename` answered, as a line and whether it is bad news:
/// "Renamed to <name>." for a name kept, "<title> is called by its title
/// page again." for a name cleared (`name` empty after trimming), the
/// engine's own sentence for a refusal, and ours when the answer does not
/// name the row's folder.
pub fn renamed_line(answer: &Value, row: &Row, name: &str) -> (String, bool) {
    if !ok(answer) {
        let line = error_message(answer).unwrap_or_else(|| NOT_RENAMED.to_string());
        return (line, true);
    }
    if text_at(answer, "folder").as_deref() != Some(row.folder.as_str()) {
        return (NOT_RENAMED.to_string(), true);
    }
    let name = name.trim();
    if !name.is_empty() {
        return (format!("Renamed to {name}."), false);
    }
    let title = text_at(answer, "title").unwrap_or_else(|| row.name.clone());
    (format!("{title} is called by its title page again."), false)
}

/// The question asked before a script goes.
pub fn question(row: &Row, windows: bool) -> String {
    format!("Move {} to the {}?", row_title(row), bin_name(windows))
}

/// The question's Move to Trash button's name for a screen reader.
pub fn move_label(row: &Row, windows: bool) -> String {
    format!("Move {} to the {}", row_title(row), bin_name(windows))
}

/// What the tab says while a script goes.
pub fn moving(row: &Row, windows: bool) -> String {
    format!("Moving {} to the {}…", row_title(row), bin_name(windows))
}

/// What `library-remove` answered, as a line and whether it is bad news:
/// the engine's own sentence for a refusal, and ours when the answer does
/// not name the row's folder as the one removed.
pub fn removed_line(answer: &Value, row: &Row, windows: bool) -> (String, bool) {
    if !ok(answer) {
        let line = error_message(answer).unwrap_or_else(|| not_removed(windows));
        return (line, true);
    }
    if text_at(answer, "removed").as_deref() != Some(row.folder.as_str()) {
        return (not_removed(windows), true);
    }
    (
        format!("{} is in the {}.", row_title(row), bin_name(windows)),
        false,
    )
}

/// The empty library's second line: where it is, with home folded to `~`.
pub fn empty_line(root: &str, home: Option<&str>) -> String {
    let shown = match home {
        Some(home) => super::convert::shorten_home(root, home),
        None => root.to_string(),
    };
    format!("Every script you convert lands in {shown}, in a folder of its own.")
}

/// The book a row's Open remakes: its `.epub`, or the one its folder would
/// hold (`<folder>/<name>.epub`), which the remaking makes.
pub fn book_path(row: &Row) -> String {
    row.epub.clone().unwrap_or_else(|| {
        Path::new(&row.folder)
            .join(format!("{}.epub", row.name))
            .to_string_lossy()
            .into_owned()
    })
}

/// A reopen's answer as a conversion's: the row's title, pages and scenes
/// (a `.fountain` conversion counts no pages), and the `.fountain` it was
/// made from.
pub fn reopened(answer: &Value, row: &Row) -> Value {
    let mut answer = answer.clone();
    if let Some(map) = answer.as_object_mut() {
        if let Some(title) = &row.title {
            map.insert("title".into(), Value::String(title.clone()));
        }
        if let Some(pages) = row.pages {
            map.insert("pages".into(), Value::from(pages));
        }
        map.insert("scenes".into(), Value::from(row.scenes));
        map.insert("fountainPath".into(), Value::String(row.fountain.clone()));
    }
    answer
}

// ---------------------------------------------------------------- report

/// The row's Report button, and what it says while its report is showing.
pub const REPORT: &str = "Report";
pub const HIDE_REPORT: &str = "Hide report";
/// The report table's column headings.
pub const REPORT_COLUMNS: [&str; 7] = [
    "Character",
    "Speeches",
    "Words",
    "Scenes",
    "First speaks",
    "Sides",
    "Lines in bold",
];
/// The button on each character's line that makes their sides.
pub const MAKE_SIDES: &str = "Make";
/// The button that shows the sides just made.
pub const SHOW_SIDES: &str = "Show the file";
/// The button that opens the book just made, to send it.
pub const SEND_MADE: &str = "Send to e-reader";
/// What the status line says when an answer says nothing usable.
pub const NOT_MADE: &str = "The sides could not be made, and nothing said why.";
/// What the status line says when a lines answer says nothing usable.
pub const LINES_NOT_MADE: &str = "The book could not be made, and nothing said why.";
/// What a report says when its answer says nothing usable.
pub const NOT_REPORTED: &str = "The report could not be made, and nothing said why.";

/// One character's line of a report, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterLine {
    pub name: String,
    pub speeches: String,
    pub words: String,
    pub scenes: String,
    /// "Scene 1, page 2", "Scene 3", or "Before the first scene".
    pub first: String,
}

/// `library-report`'s answer, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// The line of totals, as the command line's second line says it.
    pub totals: String,
    pub characters: Vec<CharacterLine>,
}

/// `1,234,567`, as the command line groups it.
pub fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn counted(n: i64, one: &str, many: &str) -> String {
    format!("{} {}", grouped(n), if n == 1 { one } else { many })
}

fn character_line(v: &Value) -> Option<CharacterLine> {
    let n = |key: &str| v.get(key)?.as_i64();
    let first_scene = optional(v, "firstScene", Value::as_i64)?;
    let first_page = optional(v, "firstPage", |x| x.as_str().map(str::to_string))?;
    let first = match (first_scene, first_page) {
        (Some(s), Some(p)) => format!("Scene {s}, page {p}"),
        (Some(s), None) => format!("Scene {s}"),
        (None, Some(p)) => format!("Before the first scene, page {p}"),
        (None, None) => "Before the first scene".to_string(),
    };
    Some(CharacterLine {
        name: text_at(v, "name")?,
        speeches: grouped(n("speeches")?),
        words: grouped(n("words")?),
        scenes: grouped(n("scenes")?),
        first,
    })
}

/// What `library-report` answered for `row`: its report, or the line that
/// says why there is none (the engine's own sentence for a refusal, ours
/// when the answer does not name the row's folder or a field is broken).
pub fn report_from(answer: &Value, row: &Row) -> Result<Report, String> {
    if !ok(answer) {
        return Err(error_message(answer).unwrap_or_else(|| NOT_REPORTED.to_string()));
    }
    let read = || {
        if text_at(answer, "folder").as_deref() != Some(row.folder.as_str()) {
            return None;
        }
        let n = |key: &str| answer.get(key)?.as_i64();
        let mut totals = Vec::new();
        if let Some(pages) = optional(answer, "pages", Value::as_i64)? {
            totals.push(counted(pages, "page", "pages"));
        }
        totals.push(counted(n("scenes")?, "scene", "scenes"));
        totals.push(counted(n("speeches")?, "speech", "speeches"));
        totals.push(format!(
            "{} of dialogue",
            counted(n("words")?, "word", "words")
        ));
        Some(Report {
            totals: totals.join(", "),
            characters: answer
                .get("characters")?
                .as_array()?
                .iter()
                .map(character_line)
                .collect::<Option<_>>()?,
        })
    };
    read().ok_or_else(|| NOT_REPORTED.to_string())
}

/// The Report button's name for a screen reader.
pub fn report_label(row: &Row) -> String {
    format!("Report for {}", row_title(row))
}

/// The Hide report button's name for a screen reader.
pub fn hide_report_label(row: &Row) -> String {
    format!("Hide the report for {}", row_title(row))
}

/// The report table's name for a screen reader.
pub fn report_table_label(row: &Row) -> String {
    format!("Who speaks in {}", row_title(row))
}

/// A book the report just made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MadeBook {
    pub epub: String,
    pub title: String,
    pub preview_html: String,
}

/// The book an answer that made one says it made, at `epub`.
pub fn made_book(answer: &Value, epub: String) -> MadeBook {
    MadeBook {
        title: text_at(answer, "title").unwrap_or_else(|| epub.clone()),
        preview_html: text_at(answer, "previewHtml").unwrap_or_default(),
        epub,
    }
}

/// A character's Make button's name for a screen reader.
pub fn make_sides_label(name: &str) -> String {
    format!("Make sides for {name}")
}

/// The status line while a character's sides are being made.
pub fn making_sides(name: &str) -> String {
    format!("Making sides for {name}…")
}

/// `library-sides`'s answer: the status line, and the book when it was
/// made for this row; otherwise why not.
pub fn sides_from(answer: &Value, row: &Row, name: &str) -> Result<(String, String), String> {
    if !ok(answer) {
        return Err(error_message(answer).unwrap_or_else(|| NOT_MADE.to_string()));
    }
    let count = |key: &str| answer.get(key).and_then(Value::as_u64);
    match (
        text_at(answer, "folder").as_deref() == Some(row.folder.as_str()),
        count("scenes"),
        count("ofScenes"),
        text_at(answer, "epub"),
    ) {
        (true, Some(scenes), Some(of), Some(epub)) => {
            let noun = if of == 1 { "scene" } else { "scenes" };
            Ok((format!("Sides for {name}: {scenes} of {of} {noun}."), epub))
        }
        _ => Err(NOT_MADE.to_string()),
    }
}

/// A character's lines-in-bold Make button's name for a screen reader.
pub fn make_lines_label(name: &str) -> String {
    format!("Make a book with {name}'s lines in bold")
}

/// The status line while that book is being made.
pub fn making_lines(name: &str) -> String {
    format!("Making a book with {name}'s lines in bold…")
}

/// `library-lines`'s answer: the status line, and the book when it was made
/// for this row; otherwise why not.
pub fn lines_from(answer: &Value, row: &Row, name: &str) -> Result<(String, String), String> {
    if !ok(answer) {
        return Err(error_message(answer).unwrap_or_else(|| LINES_NOT_MADE.to_string()));
    }
    match (
        text_at(answer, "folder").as_deref() == Some(row.folder.as_str()),
        answer.get("speeches").and_then(Value::as_u64),
        text_at(answer, "epub"),
    ) {
        (true, Some(speeches), Some(epub)) => {
            let noun = if speeches == 1 { "speech" } else { "speeches" };
            Ok((format!("{name}'s lines in bold: {speeches} {noun}."), epub))
        }
        _ => Err(LINES_NOT_MADE.to_string()),
    }
}

/// What the row says while its report is being read.
pub fn reporting(row: &Row) -> String {
    format!("Reading the report for {}…", row_title(row))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answer() -> Value {
        json!({"ok": true, "root": "/Users/ann/Method Reader", "scripts": [
            {"folder": "/Users/ann/Method Reader/Field Station", "name": "Field Station",
             "title": "FIELD STATION", "source": "/s/Field Station.pdf", "sourceExists": true,
             "fountain": "/Users/ann/Method Reader/Field Station/Field Station.fountain",
             "epub": "/Users/ann/Method Reader/Field Station/Field Station.epub",
             "pages": 18, "scenes": 24, "converted": "2026-11-02T18:00:00Z",
             "others": ["/Users/ann/Method Reader/Field Station/Field Station.azw3"],
             "settings": null},
            {"folder": "/Users/ann/Method Reader/Weather Log", "name": "Weather Log",
             "title": null, "source": "/s/Weather Log.osf", "sourceExists": false,
             "fountain": "/Users/ann/Method Reader/Weather Log/Weather Log.fountain",
             "epub": null, "pages": null, "scenes": 1, "converted": "2026-01-09T00:00:00Z",
             "others": [], "settings": "/Users/ann/Method Reader/Weather Log/Weather Log.method-reader.json"},
        ]})
    }

    #[test]
    fn the_answer_is_read_field_by_field() {
        let listing = listing_from(&answer()).expect("a list");
        assert_eq!(listing.root, "/Users/ann/Method Reader");
        let [first, second] = &listing.rows[..] else {
            panic!("two rows")
        };
        assert_eq!(first.title.as_deref(), Some("FIELD STATION"));
        assert_eq!(first.pages, Some(18));
        assert!(first.epub.is_some() && first.source_exists);
        assert_eq!(second.title, None);
        assert_eq!((second.epub.as_ref(), second.pages), (None, None));
        assert!(!second.source_exists);
        assert_eq!(first.settings, None);
        assert_eq!(
            second.settings.as_deref(),
            Some("/Users/ann/Method Reader/Weather Log/Weather Log.method-reader.json")
        );
    }

    #[test]
    fn absent_optional_fields_are_none_and_a_broken_field_breaks_the_list() {
        let mut absent = answer();
        for key in ["title", "epub", "pages", "others", "settings"] {
            absent["scripts"][1].as_object_mut().unwrap().remove(key);
        }
        let rows = listing_from(&absent).expect("still a list").rows;
        assert_eq!((rows[1].title.as_ref(), rows[1].pages), (None, None));
        assert_eq!(rows[1].settings, None);
        // A blank title is no title.
        let mut blank = answer();
        blank["scripts"][0]["title"] = json!("  ");
        assert_eq!(listing_from(&blank).unwrap().rows[0].title, None);
        for (key, bad) in [
            ("folder", json!(null)),
            ("name", json!("")),
            ("title", json!(3)),
            ("source", json!(1)),
            ("sourceExists", json!("yes")),
            ("fountain", json!(null)),
            ("epub", json!(false)),
            ("pages", json!("18")),
            ("scenes", json!(null)),
            ("converted", json!(7)),
            ("settings", json!(true)),
        ] {
            let mut broken = answer();
            broken["scripts"][0][key] = bad;
            assert_eq!(listing_from(&broken), None, "{key}");
        }
        let mut no_list = answer();
        no_list.as_object_mut().unwrap().remove("scripts");
        assert_eq!(listing_from(&no_list), None);
        let refused =
            json!({"ok": false, "error": {"code": "internal", "message": "No library there."}});
        assert_eq!(listing_from(&refused), None);
        assert_eq!(listing_failure(&refused), "No library there.");
        assert_eq!(listing_failure(&json!({})), UNREADABLE);
        // An empty library is a list.
        let empty = json!({"ok": true, "root": "/r", "scripts": []});
        assert!(listing_from(&empty).unwrap().rows.is_empty());
    }

    #[test]
    fn a_row_says_its_title_counts_and_day_plainly() {
        let rows = listing_from(&answer()).unwrap().rows;
        assert_eq!(row_title(&rows[0]), "FIELD STATION");
        assert_eq!(row_title(&rows[1]), "Weather Log");
        assert_eq!(
            detail(&rows[0]),
            "18 pages, 24 scenes, converted 2 November 2026"
        );
        assert_eq!(
            detail(&rows[1]),
            "1 scene, converted 9 January 2026, its own settings"
        );
        let one_page = Row {
            pages: Some(1),
            converted: "not a date".into(),
            ..rows[0].clone()
        };
        assert_eq!(detail(&one_page), "1 page, 24 scenes");
        assert_eq!(open_label(&rows[0]), "Open FIELD STATION");
        assert_eq!(SETTINGS, "Settings");
        assert_eq!(settings_label(&rows[0]), "Settings for FIELD STATION");
        assert_eq!(settings_label(&rows[1]), "Settings for Weather Log");
        assert_eq!(gone_note(&rows[0]), None);
        assert_eq!(
            gone_note(&rows[1]),
            Some("The file it came from has moved or gone; the book still opens.")
        );
        let gone_pdf = Row {
            source_exists: false,
            ..rows[0].clone()
        };
        assert_eq!(
            gone_note(&gone_pdf),
            Some("Its PDF has moved or gone; the book still opens.")
        );
    }

    #[test]
    fn days_are_words_or_nothing() {
        assert_eq!(
            date_words("2026-11-02T18:00:00Z").as_deref(),
            Some("2 November 2026")
        );
        assert_eq!(date_words("2026-13-02T00:00:00Z"), None);
        assert_eq!(date_words("2026-00-02T00:00:00Z"), None);
        assert_eq!(date_words("2026-11-32"), None);
        assert_eq!(date_words("soon"), None);
        assert_eq!(date_words(""), None);
    }

    #[test]
    fn remove_asks_in_plain_words_and_answers_with_the_title() {
        let rows = listing_from(&answer()).unwrap().rows;
        let (field, log) = (&rows[0], &rows[1]);
        assert_eq!(REMOVE, "Remove from library");
        assert_eq!(remove_label(field), "Remove FIELD STATION from the library");
        assert_eq!(remove_label(log), "Remove Weather Log from the library");
        assert_eq!(CANCEL, "Cancel");
        // Every word about the Trash, on a Mac or Linux and then on
        // Windows.
        for (windows, bin) in [(false, "Trash"), (true, "Recycle Bin")] {
            assert_eq!(bin_name(windows), bin);
            assert_eq!(
                question(field, windows),
                format!("Move FIELD STATION to the {bin}?")
            );
            assert_eq!(
                question_note(windows),
                format!(
                    "Its folder, book and settings go to the {bin}, where you can put them back. The PDF stays where it is."
                )
            );
            assert_eq!(move_button(windows), format!("Move to {bin}"));
            assert_eq!(
                move_label(log, windows),
                format!("Move Weather Log to the {bin}")
            );
            assert_eq!(
                moving(field, windows),
                format!("Moving FIELD STATION to the {bin}…")
            );
            let not = format!("The script was not moved to the {bin}, and nothing said why.");
            assert_eq!(not_removed(windows), not);
            let gone = json!({"ok": true, "removed": field.folder, "name": "Field Station"});
            assert_eq!(
                removed_line(&gone, field, windows),
                (format!("FIELD STATION is in the {bin}."), false)
            );
            // An answer about another folder, or none, is not this row gone.
            assert_eq!(removed_line(&gone, log, windows), (not.clone(), true));
            assert_eq!(
                removed_line(&json!({"ok": true}), field, windows),
                (not.clone(), true)
            );
            let refused = json!({"ok": false, "error": {"code": "not-a-script",
                "message": "not a script in the library (it is a link): /x"}});
            assert_eq!(
                removed_line(&refused, field, windows),
                (
                    "not a script in the library (it is a link): /x".to_string(),
                    true
                )
            );
            assert_eq!(
                removed_line(&json!({"ok": false}), field, windows),
                (not.clone(), true)
            );
            // No exclamation mark in any of it (tests/no_em_dash.rs reads
            // the dashes).
            for words in [
                REMOVE.to_string(),
                SETTINGS.to_string(),
                CANCEL.to_string(),
                move_button(windows),
                question_note(windows),
                not,
                question(field, windows),
                moving(field, windows),
                remove_label(field),
                move_label(field, windows),
                settings_label(field),
                detail(log),
            ] {
                assert!(!words.contains('!'), "{words}");
            }
        }
    }

    #[test]
    fn rename_says_the_new_name_or_that_the_title_page_is_back() {
        let rows = listing_from(&answer()).unwrap().rows;
        let (field, log) = (&rows[0], &rows[1]);
        assert_eq!((RENAME, SAVE), ("Rename", "Save"));
        assert_eq!(rename_label(field), "Rename FIELD STATION");
        assert_eq!(rename_label(log), "Rename Weather Log");
        assert_eq!(name_field_label(field), "New name for FIELD STATION");
        assert_eq!(save_label(log), "Save the new name for Weather Log");
        assert_eq!(renaming(field), "Renaming FIELD STATION…");
        let kept = json!({"ok": true, "folder": field.folder, "title": "Harbour Light",
            "epub": "/b.epub"});
        assert_eq!(
            renamed_line(&kept, field, " Harbour Light "),
            ("Renamed to Harbour Light.".to_string(), false)
        );
        let cleared = json!({"ok": true, "folder": field.folder, "title": "FIELD STATION",
            "epub": "/b.epub"});
        assert_eq!(
            renamed_line(&cleared, field, "  "),
            (
                "FIELD STATION is called by its title page again.".to_string(),
                false
            )
        );
        // An answer about another folder, or a refusal, is not this row
        // renamed.
        assert_eq!(
            renamed_line(&kept, log, "Harbour Light"),
            (NOT_RENAMED.to_string(), true)
        );
        let refused = json!({"ok": false, "error": {"code": "usage",
            "message": "a script's name is one line: it cannot hold a line break"}});
        assert_eq!(
            renamed_line(&refused, field, "x"),
            (
                "a script's name is one line: it cannot hold a line break".to_string(),
                true
            )
        );
        assert_eq!(
            renamed_line(&json!({"ok": false}), field, "x"),
            (NOT_RENAMED.to_string(), true)
        );
        for words in [
            RENAME.to_string(),
            SAVE.to_string(),
            NOT_RENAMED.to_string(),
            rename_label(field),
            name_field_label(field),
            save_label(field),
            renaming(field),
            renamed_line(&kept, field, "Harbour Light").0,
            renamed_line(&cleared, field, "").0,
        ] {
            assert!(!words.contains('!'), "{words}");
        }
    }

    #[test]
    fn the_empty_library_says_where_it_is() {
        assert_eq!(
            empty_line("/Users/ann/Method Reader", Some("/Users/ann")),
            "Every script you convert lands in ~/Method Reader, in a folder of its own."
        );
        assert_eq!(
            empty_line("/srv/books", None),
            "Every script you convert lands in /srv/books, in a folder of its own."
        );
    }

    #[test]
    fn open_remakes_the_book_and_answers_as_a_conversion() {
        let rows = listing_from(&answer()).unwrap().rows;
        assert_eq!(
            book_path(&rows[0]),
            "/Users/ann/Method Reader/Field Station/Field Station.epub"
        );
        assert_eq!(
            book_path(&rows[1]),
            Path::new("/Users/ann/Method Reader/Weather Log")
                .join("Weather Log.epub")
                .to_string_lossy()
        );
        let converted = json!({"ok": true, "title": "Field Station", "epubPath": "/b.epub"});
        let shown = reopened(&converted, &rows[0]);
        assert_eq!(shown["title"], "FIELD STATION");
        assert_eq!(
            (shown["pages"].as_i64(), shown["scenes"].as_i64()),
            (Some(18), Some(24))
        );
        assert_eq!(shown["fountainPath"], rows[0].fountain.as_str());
        assert_eq!(shown["epubPath"], "/b.epub");
        // No title and no pages: the engine's own title stays, and no
        // page count is made up.
        let shown = reopened(&converted, &rows[1]);
        assert_eq!(shown["title"], "Field Station");
        assert!(shown.get("pages").is_none());
    }

    fn the_row() -> Row {
        listing_from(&answer()).unwrap().rows.remove(0)
    }

    #[test]
    fn a_report_reads_as_the_command_line_says_it() {
        let row = the_row();
        let answer = json!({"ok": true, "folder": row.folder, "title": "FIELD STATION",
        "pages": 18, "scenes": 1, "speeches": 1204, "words": 1,
        "characters": [
            {"name": "MARA", "speeches": 1200, "words": 1, "scenes": 1,
                "firstScene": 1, "firstPage": "2"},
            {"name": "OTTO", "speeches": 3, "words": 0, "scenes": 1,
                "firstScene": 1, "firstPage": null},
            {"name": "RADIO", "speeches": 1, "words": 0, "scenes": 0,
                "firstScene": null, "firstPage": "1"},
            {"name": "VOICE", "speeches": 0, "words": 0, "scenes": 0,
                "firstScene": null, "firstPage": null},
        ]});
        let report = report_from(&answer, &row).unwrap();
        assert_eq!(
            report.totals,
            "18 pages, 1 scene, 1,204 speeches, 1 word of dialogue"
        );
        let first: Vec<&str> = report.characters.iter().map(|c| c.first.as_str()).collect();
        assert_eq!(
            first,
            [
                "Scene 1, page 2",
                "Scene 1",
                "Before the first scene, page 1",
                "Before the first scene"
            ]
        );
        assert_eq!(report.characters[0].speeches, "1,200");

        let mut no_pages = answer.clone();
        no_pages["pages"] = Value::Null;
        assert!(
            report_from(&no_pages, &row)
                .unwrap()
                .totals
                .starts_with("1 scene, ")
        );
    }

    #[test]
    fn a_report_that_is_not_this_row_s_or_is_broken_says_so() {
        let row = the_row();
        let refused = json!({"ok": false, "error": {"code": "not-a-script", "message": "No."}});
        assert_eq!(report_from(&refused, &row), Err("No.".to_string()));
        let other = json!({"ok": true, "folder": "/elsewhere", "scenes": 1, "speeches": 0,
            "words": 0, "characters": []});
        assert_eq!(report_from(&other, &row), Err(NOT_REPORTED.to_string()));
        let broken = json!({"ok": true, "folder": row.folder, "scenes": 1, "speeches": 0,
            "words": 0, "characters": [{"name": "MARA"}]});
        assert_eq!(report_from(&broken, &row), Err(NOT_REPORTED.to_string()));
    }

    #[test]
    fn numbers_group_by_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }
}
