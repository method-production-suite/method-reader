//! The Library tab (docs/superpowers/specs/2026-11-02-library-view-design.md),
//! as the window decides it: the engine's `library` answer read field by
//! field, the words each row says, the empty library's words, how a
//! row's Open is asked and answered, and the question and answer of its
//! Remove from library (docs/superpowers/specs/2026-11-03-library-remove-design.md).
//! No toolkit in it; the drawing is src/app/library.rs.

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
/// The question's two answers.
pub const MOVE: &str = "Move to Trash";
pub const CANCEL: &str = "Cancel";
/// What the question says goes, and what stays.
pub const QUESTION_NOTE: &str = "Its folder, book and settings go to the Trash, where you can put them back. The PDF stays where it is.";
pub const NOT_REMOVED: &str = "The script was not moved to the Trash, and nothing said why.";

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
/// them), its scenes, and the day it was converted.
pub fn detail(row: &Row) -> String {
    let mut parts = Vec::new();
    if let Some(pages) = row.pages {
        parts.push(count(pages, "page", "pages"));
    }
    parts.push(count(row.scenes, "scene", "scenes"));
    if let Some(day) = date_words(&row.converted) {
        parts.push(format!("converted {day}"));
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

/// The question asked before a script goes.
pub fn question(row: &Row) -> String {
    format!("Move {} to the Trash?", row_title(row))
}

/// The question's Move to Trash button's name for a screen reader.
pub fn move_label(row: &Row) -> String {
    format!("Move {} to the Trash", row_title(row))
}

/// What the tab says while a script goes.
pub fn moving(row: &Row) -> String {
    format!("Moving {} to the Trash…", row_title(row))
}

/// What `library-remove` answered, as a line and whether it is bad news:
/// the engine's own sentence for a refusal, and ours when the answer does
/// not name the row's folder as the one removed.
pub fn removed_line(answer: &Value, row: &Row) -> (String, bool) {
    if !ok(answer) {
        let line = error_message(answer).unwrap_or_else(|| NOT_REMOVED.to_string());
        return (line, true);
    }
    if text_at(answer, "removed").as_deref() != Some(row.folder.as_str()) {
        return (NOT_REMOVED.to_string(), true);
    }
    (format!("{} is in the Trash.", row_title(row)), false)
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
             "others": [], "settings": null},
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
    }

    #[test]
    fn absent_optional_fields_are_none_and_a_broken_field_breaks_the_list() {
        let mut absent = answer();
        for key in ["title", "epub", "pages", "others", "settings"] {
            absent["scripts"][1].as_object_mut().unwrap().remove(key);
        }
        let rows = listing_from(&absent).expect("still a list").rows;
        assert_eq!((rows[1].title.as_ref(), rows[1].pages), (None, None));
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
        assert_eq!(detail(&rows[1]), "1 scene, converted 9 January 2026");
        let one_page = Row {
            pages: Some(1),
            converted: "not a date".into(),
            ..rows[0].clone()
        };
        assert_eq!(detail(&one_page), "1 page, 24 scenes");
        assert_eq!(open_label(&rows[0]), "Open FIELD STATION");
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
        assert_eq!(question(field), "Move FIELD STATION to the Trash?");
        assert_eq!(
            QUESTION_NOTE,
            "Its folder, book and settings go to the Trash, where you can put them back. The PDF stays where it is."
        );
        assert_eq!((MOVE, CANCEL), ("Move to Trash", "Cancel"));
        assert_eq!(move_label(log), "Move Weather Log to the Trash");
        assert_eq!(moving(field), "Moving FIELD STATION to the Trash…");
        let gone = json!({"ok": true, "removed": field.folder, "name": "Field Station"});
        assert_eq!(
            removed_line(&gone, field),
            ("FIELD STATION is in the Trash.".to_string(), false)
        );
        // An answer about another folder, or none, is not this row gone.
        assert_eq!(removed_line(&gone, log), (NOT_REMOVED.to_string(), true));
        assert_eq!(
            removed_line(&json!({"ok": true}), field),
            (NOT_REMOVED.to_string(), true)
        );
        let refused = json!({"ok": false, "error": {"code": "not-a-script",
            "message": "not a script in the library (it is a link): /x"}});
        assert_eq!(
            removed_line(&refused, field),
            (
                "not a script in the library (it is a link): /x".to_string(),
                true
            )
        );
        assert_eq!(
            removed_line(&json!({"ok": false}), field),
            (NOT_REMOVED.to_string(), true)
        );
        // No exclamation mark in any of it (tests/no_em_dash.rs reads the
        // dashes).
        for words in [
            REMOVE,
            MOVE,
            CANCEL,
            QUESTION_NOTE,
            NOT_REMOVED,
            &question(field),
            &moving(field),
            &remove_label(field),
            &move_label(field),
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
}
