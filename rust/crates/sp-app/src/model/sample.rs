//! The sample: Field Station, the invented script every README and site
//! picture shows (SUITE-STANDARDS.md, "The Field Station showcase"), built
//! into the window so the first screen can show a book.
//!
//! The bundled PDF is the committed fixture, tests/fixtures/field-station.pdf,
//! byte for byte (a test holds the two equal). It is converted through the
//! engine's ordinary conversion into a folder of its own in the app-data
//! folder, never into the person's library, so their library holds only
//! what they converted. The first launch of a fresh install opens it once,
//! unless the switch on the Settings page is off; Help, Open Sample opens it
//! any time, and reopens the book already made when it is current rather
//! than converting again.
//!
//! What is decided lives here; the window's half (the disk and the calls)
//! is src/app/sample.rs.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::update::{Storage, ran_before};

/// The sample PDF, built in.
pub const PDF: &[u8] = include_bytes!("../../../../../tests/fixtures/field-station.pdf");

/// The sample's folder, inside the app-data folder.
pub const FOLDER: &str = "Sample";
/// What the sample's files are called, before the extension.
pub const STEM: &str = "Field Station";

/// The switch, once a person has set it: "true" or "false". Absent, on.
const SWITCH: &str = "sampleFirstLaunch";
/// Set once the first launch has had its turn, whatever it did.
const FIRST_LAUNCH_DONE: &str = "sampleFirstLaunchDone";

/// The Help menu's item.
pub const MENU_LABEL: &str = "Open Sample";
/// The Settings page's group, and its one switch.
pub const SWITCH_LEGEND: &str = "When Method Reader opens";
pub const SWITCH_LABEL: &str = "Open the sample the first time";
pub const SWITCH_LINE: &str = "The first time the window opens with nothing in it, it shows Field Station, an invented script, as a book. Open Sample in the Help menu shows it any time.";
/// The quiet line beside the sample's book.
pub const NOTE: &str = "This is Field Station, an invented sample script. To make a book of your own, choose a screenplay on the Convert page.";

/// Where the sample's files go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub dir: PathBuf,
    pub pdf: String,
    pub epub: String,
    pub fountain: String,
    /// What the last conversion of the sample said, kept beside it so a
    /// reopen can say it again (a `.fountain` conversion has no page count).
    pub stamp: String,
}

impl Paths {
    pub fn in_folder(dir: &Path) -> Self {
        let file = |ext: &str| {
            dir.join(format!("{STEM}{ext}"))
                .to_string_lossy()
                .into_owned()
        };
        Self {
            dir: dir.to_path_buf(),
            pdf: file(".pdf"),
            epub: file(".epub"),
            fountain: file(".fountain"),
            stamp: file(".sample.json"),
        }
    }
}

/// Whether the first launch opens the sample: the switch.
pub fn first_launch_on(storage: &dyn Storage) -> bool {
    storage.get_item(SWITCH).as_deref() != Some("false")
}

/// The switch, set by a person.
pub fn set_first_launch(storage: &mut dyn Storage, on: bool) {
    storage.set_item(SWITCH, if on { "true" } else { "false" });
}

/// Whether this launch is the first, read when the window is made (before
/// What's New writes its mark): a fresh install, the switch on, and the
/// first launch not yet had. Written down at once, so it happens once.
pub fn first_launch_due(storage: &mut dyn Storage) -> bool {
    if storage.get_item(FIRST_LAUNCH_DONE).as_deref() == Some("true") {
        return false;
    }
    let due = !ran_before(storage) && first_launch_on(storage);
    storage.set_item(FIRST_LAUNCH_DONE, "true");
    due
}

/// What is on disk in the sample's folder, as the window found it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OnDisk {
    /// The PDF there is the one built in.
    pub pdf_is_bundled: bool,
    /// The book is there.
    pub book: bool,
    /// The `.fountain` is there, and no older than the PDF.
    pub fountain_fresh: bool,
    /// The stamp's contents, when it reads as JSON.
    pub stamp: Option<Value>,
}

/// What opening the sample does.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Convert the PDF.
    Convert,
    /// Make the book from the `.fountain` already there (the person's own
    /// Settings for the sample kept), and say what the stamp says.
    Reopen { stamp: Value },
}

/// Reopen only when the `.fountain` is current: made from this PDF, no
/// older than it, by this version of the engine (a newer engine reads the
/// PDF afresh), with its book beside it.
pub fn plan(disk: &OnDisk, running: &str) -> Plan {
    let stamp = disk
        .stamp
        .as_ref()
        .filter(|s| s.get("version").and_then(Value::as_str) == Some(running));
    match stamp {
        Some(stamp) if disk.pdf_is_bundled && disk.book && disk.fountain_fresh => Plan::Reopen {
            stamp: stamp.clone(),
        },
        _ => Plan::Convert,
    }
}

/// The stamp a conversion of the PDF leaves: the version that made it and
/// the facts a reopen cannot learn from the `.fountain`.
pub fn stamp_from(answer: &Value, running: &str) -> Value {
    let mut stamp = json!({ "version": running });
    for key in ["title", "author", "pages", "scenes"] {
        if let Some(v) = answer.get(key).filter(|v| !v.is_null()) {
            stamp[key] = v.clone();
        }
    }
    stamp
}

/// A reopen's answer as a conversion's: the stamp's facts, and the
/// `.fountain` it was made from.
pub fn reopened(answer: &Value, stamp: &Value, fountain: &str) -> Value {
    let mut answer = answer.clone();
    if let Some(map) = answer.as_object_mut() {
        for key in ["title", "author", "pages", "scenes"] {
            if let Some(v) = stamp.get(key) {
                map.insert(key.into(), v.clone());
            }
        }
        map.insert("fountainPath".into(), Value::String(fountain.into()));
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::update::MemoryStorage;

    fn disk(stamp: Option<Value>) -> OnDisk {
        OnDisk {
            pdf_is_bundled: true,
            book: true,
            fountain_fresh: true,
            stamp,
        }
    }

    #[test]
    fn the_bundled_pdf_is_the_committed_fixture() {
        let fixture = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/fixtures/field-station.pdf"
        ))
        .expect("the committed fixture");
        assert!(PDF == fixture.as_slice(), "the built-in sample drifted");
    }

    #[test]
    fn the_files_sit_in_the_sample_folder_under_the_script_s_name() {
        let p = Paths::in_folder(Path::new("/data/Sample"));
        assert_eq!(p.pdf, "/data/Sample/Field Station.pdf");
        assert_eq!(p.epub, "/data/Sample/Field Station.epub");
        assert_eq!(p.fountain, "/data/Sample/Field Station.fountain");
        assert_eq!(p.stamp, "/data/Sample/Field Station.sample.json");
    }

    #[test]
    fn a_fresh_install_opens_it_once() {
        let mut s = MemoryStorage::default();
        assert!(first_launch_on(&s), "on by default");
        assert!(first_launch_due(&mut s));
        assert!(!first_launch_due(&mut s), "once");
    }

    #[test]
    fn the_switch_off_or_an_earlier_copy_opens_nothing() {
        let mut off = MemoryStorage::default();
        set_first_launch(&mut off, false);
        assert!(!first_launch_on(&off));
        assert!(!first_launch_due(&mut off));
        // Turned back on later, the first launch has already had its turn.
        set_first_launch(&mut off, true);
        assert!(!first_launch_due(&mut off));

        let mut updated = MemoryStorage::default();
        updated.set_item("whatsNewSeen", "0.13.0");
        assert!(
            !first_launch_due(&mut updated),
            "an update is no first launch"
        );
        let mut old = MemoryStorage::default();
        old.set_item("updateAsked", "true");
        assert!(!first_launch_due(&mut old));
    }

    #[test]
    fn it_reopens_only_what_this_engine_made_from_this_pdf() {
        let stamp = json!({"version": "1.0.0", "title": "Field Station", "pages": 19});
        assert_eq!(
            plan(&disk(Some(stamp.clone())), "1.0.0"),
            Plan::Reopen {
                stamp: stamp.clone()
            }
        );
        assert_eq!(plan(&disk(Some(stamp.clone())), "1.0.1"), Plan::Convert);
        assert_eq!(plan(&disk(None), "1.0.0"), Plan::Convert);
        for broken in [
            OnDisk {
                pdf_is_bundled: false,
                ..disk(Some(stamp.clone()))
            },
            OnDisk {
                book: false,
                ..disk(Some(stamp.clone()))
            },
            OnDisk {
                fountain_fresh: false,
                ..disk(Some(stamp.clone()))
            },
        ] {
            assert_eq!(plan(&broken, "1.0.0"), Plan::Convert, "{broken:?}");
        }
    }

    #[test]
    fn a_reopen_says_what_the_conversion_said() {
        let converted = json!({"ok": true, "title": "Field Station", "author": "A", "pages": 19, "scenes": 11, "epubPath": "/s/F.epub"});
        let stamp = stamp_from(&converted, "1.0.0");
        assert_eq!(
            stamp,
            json!({"version": "1.0.0", "title": "Field Station", "author": "A", "pages": 19, "scenes": 11})
        );
        let answer = reopened(
            &json!({"ok": true, "title": "Untitled", "epubPath": "/s/F.epub"}),
            &stamp,
            "/s/F.fountain",
        );
        assert_eq!(answer["title"], "Field Station");
        assert_eq!(answer["pages"], 19);
        assert_eq!(answer["fountainPath"], "/s/F.fountain");
        assert_eq!(answer["epubPath"], "/s/F.epub");
    }
}
