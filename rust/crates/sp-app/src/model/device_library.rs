//! What is on the reader (docs/device-library.md), as the window decides
//! it: which connected readers can be looked at, the engine's
//! `device-books` answer read field by field, the words each row and each
//! step says, and what `device-remove` answered. No toolkit in it.

use serde_json::Value;

use super::json::{error_message, ok, text_at};
use super::send_routes::{Device, Routes};

/// The kinds whose books can be listed and removed: the ones reached as a
/// volume. A reMarkable's interface has no delete, and a Kindle over MTP
/// waits for the MTP helper.
pub const LISTABLE: [&str; 3] = ["kindle", "kobo", "tolino"];

/// The books Method Reader made from a script in the library.
pub const YOUR_SCRIPTS: &str = "Your scripts";
pub const OTHER_BOOKS: &str = "Other books";
pub const REMOVE: &str = "Remove";
pub const CANCEL: &str = "Cancel";
/// What goes with a book on a Kindle, said before it goes.
pub const KINDLE_NOTES: &str = "Its reading place, highlights and notes go with it.";
pub const UNREADABLE: &str = "The list of books could not be read.";
pub const NOT_REMOVED: &str = "The book was not removed, and nothing said why.";

/// Whether this reader's books can be listed: a volume the engine found,
/// of a kind that keeps its books in a folder.
pub fn listable(device: &Device) -> bool {
    LISTABLE.contains(&device.kind.as_str())
        && device.transport.is_none()
        && device
            .volume
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty())
}

/// The readers to offer a list for: the devices poll first, then any the
/// route list carries, each volume once.
pub fn readers(devices: Option<&[Device]>, routes: Option<&Routes>) -> Vec<Device> {
    let polled = devices.into_iter().flatten().cloned();
    let routed = routes
        .into_iter()
        .flat_map(|r| r.routes.iter())
        .filter(|r| r.available)
        .filter_map(|r| r.device.clone());
    let mut out: Vec<Device> = Vec::new();
    for device in polled.chain(routed) {
        if listable(&device) && !out.iter().any(|d| d.volume == device.volume) {
            out.push(device);
        }
    }
    out
}

/// Whether a reader is still in the list (by volume).
pub fn still_there(device: &Device, list: &[Device]) -> bool {
    list.iter().any(|d| d.volume == device.volume)
}

/// The button that opens the list.
pub fn open_label(device: &Device) -> String {
    format!("Books on your {}", device.name)
}

/// The list's heading.
pub fn heading(device: &Device) -> String {
    format!("On your {}", device.name)
}

pub fn looking(device: &Device) -> String {
    format!("Looking at your {}…", device.name)
}

pub fn empty(device: &Device) -> String {
    format!("No books on your {}.", device.name)
}

/// One book on the reader, as the engine listed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    pub path: String,
    /// Its path inside the reader's books folder.
    pub name: String,
    pub size: u64,
    pub format: String,
    /// The library folder holding the same bytes, when there is one.
    pub library: Option<String>,
}

/// `device-books`' answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shelf {
    pub volume: String,
    pub kind: String,
    pub books: Vec<Book>,
}

impl Shelf {
    /// The books Method Reader made, then the rest, each in the engine's
    /// order.
    pub fn groups(&self) -> (Vec<&Book>, Vec<&Book>) {
        self.books.iter().partition(|b| b.library.is_some())
    }
}

fn book_from(v: &Value) -> Option<Book> {
    let library = match v.get("library") {
        None | Some(Value::Null) => None,
        Some(l) => Some(l.as_str()?.trim().to_string()).filter(|s| !s.is_empty()),
    };
    Some(Book {
        path: text_at(v, "path")?,
        name: text_at(v, "name")?,
        size: v.get("size")?.as_u64()?,
        format: text_at(v, "format")?,
        library,
    })
}

/// The answer, when it is one; a book with a broken field breaks the list,
/// as every list the window reads does.
pub fn shelf_from(answer: &Value) -> Option<Shelf> {
    if !ok(answer) {
        return None;
    }
    Some(Shelf {
        volume: text_at(answer, "volume")?,
        kind: text_at(answer, "kind")?,
        books: answer
            .get("books")?
            .as_array()?
            .iter()
            .map(book_from)
            .collect::<Option<_>>()?,
    })
}

/// Why there is no list: the engine's own sentence, or ours.
pub fn shelf_failure(answer: &Value) -> String {
    error_message(answer).unwrap_or_else(|| UNREADABLE.to_string())
}

/// A file's size in words: bytes, KB under a megabyte, MB with one place.
pub fn size_words(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        format!("{bytes} bytes")
    } else if bytes < MB {
        format!("{} KB", (bytes as f64 / KB as f64).round() as u64)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}

/// The line under a book's name: its format, its size, and the library
/// folder it came from, when it did.
pub fn detail(book: &Book) -> String {
    let mut line = format!("{}, {}", book.format.to_uppercase(), size_words(book.size));
    if let Some(script) = &book.library {
        line.push_str(&format!(", from {script}"));
    }
    line
}

/// The remove button's name for a screen reader.
pub fn remove_label(book: &Book) -> String {
    format!("Remove {}…", book.name)
}

/// The question asked before a book goes.
pub fn question(book: &Book, device: &Device) -> String {
    format!("Remove {} from your {}?", book.name, device.name)
}

/// What else the question says, for this reader.
pub fn question_note(device: &Device) -> Option<&'static str> {
    (device.kind == "kindle").then_some(KINDLE_NOTES)
}

pub fn removing(book: &Book) -> String {
    format!("Removing {}…", book.name)
}

/// What `device-remove` answered, as a line and whether it is bad news.
pub fn removed_line(answer: &Value, book: &Book, device: &Device) -> (String, bool) {
    if !ok(answer) {
        let line = error_message(answer).unwrap_or_else(|| NOT_REMOVED.to_string());
        return (line, true);
    }
    let gone: Vec<&str> = answer
        .get("removed")
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !gone.contains(&book.path.as_str()) {
        return (NOT_REMOVED.to_string(), true);
    }
    let notes = gone.iter().any(|g| g.ends_with(".sdr"));
    let what = if notes {
        format!("{} and its notes", book.name)
    } else {
        book.name.clone()
    };
    (format!("Removed {what} from your {}.", device.name), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn kindle() -> Device {
        Device {
            id: "d1".into(),
            kind: "kindle".into(),
            name: "Kindle".into(),
            volume: Some("/Volumes/Kindle".into()),
            transport: None,
            copy_note: None,
        }
    }

    fn answer() -> Value {
        json!({"ok": true, "volume": "/Volumes/Kindle", "kind": "kindle",
            "folder": "/Volumes/Kindle/documents", "books": [
            {"path": "/Volumes/Kindle/documents/Reading list.pdf", "name": "Reading list.pdf",
             "size": 2_400_000, "format": "pdf"},
            {"path": "/Volumes/Kindle/documents/Field Station.azw3", "name": "Field Station.azw3",
             "size": 245_760, "format": "azw3", "library": "Field Station"},
        ]})
    }

    #[test]
    fn only_a_mounted_kindle_kobo_or_tolino_is_listable() {
        assert!(listable(&kindle()));
        let mtp = Device {
            transport: Some("mtp".into()),
            ..kindle()
        };
        assert!(!listable(&mtp));
        let remarkable = Device {
            kind: "remarkable".into(),
            ..kindle()
        };
        assert!(!listable(&remarkable));
        let no_volume = Device {
            volume: None,
            ..kindle()
        };
        assert!(!listable(&no_volume));
    }

    #[test]
    fn each_reader_is_offered_once() {
        let list = [kindle(), kindle()];
        assert_eq!(readers(Some(&list), None).len(), 1);
        assert!(readers(None, None).is_empty());
    }

    #[test]
    fn the_answer_is_read_field_by_field_and_the_librarys_come_first() {
        let shelf = shelf_from(&answer()).expect("a list");
        assert_eq!(shelf.books.len(), 2);
        let (ours, others) = shelf.groups();
        assert_eq!(ours[0].name, "Field Station.azw3");
        assert_eq!(ours[0].library.as_deref(), Some("Field Station"));
        assert_eq!(others[0].name, "Reading list.pdf");
        let mut broken = answer();
        broken["books"][0]["size"] = json!("big");
        assert_eq!(shelf_from(&broken), None);
        let refused = json!({"ok": false, "error": {"code": "unknown-device", "message": "No reader there."}});
        assert_eq!(shelf_from(&refused), None);
        assert_eq!(shelf_failure(&refused), "No reader there.");
        assert_eq!(shelf_failure(&json!({})), UNREADABLE);
    }

    #[test]
    fn sizes_and_lines_are_plain() {
        assert_eq!(size_words(900), "900 bytes");
        assert_eq!(size_words(245_760), "240 KB");
        assert_eq!(size_words(2_400_000), "2.3 MB");
        let shelf = shelf_from(&answer()).unwrap();
        assert_eq!(detail(&shelf.books[1]), "AZW3, 240 KB, from Field Station");
        assert_eq!(detail(&shelf.books[0]), "PDF, 2.3 MB");
        assert_eq!(open_label(&kindle()), "Books on your Kindle");
        assert_eq!(
            question(&shelf.books[1], &kindle()),
            "Remove Field Station.azw3 from your Kindle?"
        );
        assert_eq!(question_note(&kindle()), Some(KINDLE_NOTES));
        let kobo = Device {
            kind: "kobo".into(),
            name: "Kobo".into(),
            ..kindle()
        };
        assert_eq!(question_note(&kobo), None);
    }

    #[test]
    fn what_was_removed_is_said_with_its_notes() {
        let shelf = shelf_from(&answer()).unwrap();
        let book = &shelf.books[1];
        let both = json!({"ok": true, "removed": [book.path, "/Volumes/Kindle/documents/Field Station.sdr"]});
        assert_eq!(
            removed_line(&both, book, &kindle()),
            (
                "Removed Field Station.azw3 and its notes from your Kindle.".into(),
                false
            )
        );
        let alone = json!({"ok": true, "removed": [book.path]});
        assert_eq!(
            removed_line(&alone, book, &kindle()).0,
            "Removed Field Station.azw3 from your Kindle."
        );
        let failed = json!({"ok": false, "error": {"code": "remove-failed",
            "message": "The book was removed, but its notes folder would not go."}});
        assert_eq!(
            removed_line(&failed, book, &kindle()),
            (
                "The book was removed, but its notes folder would not go.".into(),
                true
            )
        );
        let other = json!({"ok": true, "removed": ["/elsewhere"]});
        assert!(removed_line(&other, book, &kindle()).1);
    }
}
