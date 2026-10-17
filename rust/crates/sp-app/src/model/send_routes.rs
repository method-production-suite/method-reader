//! What sending decides, with no drawing in it (was
//! `desktop/ui/send-routes.js`).

use serde_json::{Map, Value};

use super::convert::{Platform, Script};
use super::json::{error_message, is_text, ok, str_at};

pub const UNPROVEN: &str = "never been tested on real hardware";

/// The one combination anyone has actually run: a Kindle on a Mac.
pub const PROVEN_KIND: &str = "kindle";
pub const PROVEN_PLATFORM: Platform = Platform::Mac;

/// A reader Method Reader could not find even if it were plugged in.
pub fn undetectable(platform: Platform) -> Option<String> {
    (platform == Platform::Windows).then(|| {
        format!(
            "A tolino cannot be found on Windows at all: it is identified by the name of its volume, and a Windows drive root carries none. Windows drive detection as a whole has {UNPROVEN}."
        )
    })
}

/// A connected device as a route row carries it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Device {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub volume: Option<String>,
    /// "mtp" for a Kindle reached over MTP, the one transport the engine
    /// names; None for a mounted volume.
    pub transport: Option<String>,
    /// The engine's words for while a copy to it runs (camera import
    /// paused, on a Mac).
    pub copy_note: Option<String>,
}

/// The end of a copy to a reader that never mounted (a Kindle over MTP).
pub const SAFE_TO_UNPLUG: &str = "It is safe to unplug.";

/// `withTransport(clean, device)`: the two fields a Kindle over MTP
/// carries, when the engine sent them; anything else is left behind.
/// Shared by the route list and the devices poll.
pub fn with_transport(mut clean: Device, raw: &Value) -> Device {
    if raw.get("transport").and_then(Value::as_str) == Some("mtp") {
        clean.transport = Some("mtp".into());
        if let Some(note) = raw.get("copyNote").filter(|v| is_text(Some(v))) {
            clean.copy_note = note.as_str().map(str::to_string);
        }
    }
    clean
}

/// Whether a reader wears the Beta badge: a Kindle reached over MTP.
pub fn is_beta_device(device: Option<&Device>) -> bool {
    device.is_some_and(|d| d.transport.as_deref() == Some("mtp"))
}

/// The words for while a copy to this reader runs, or None.
pub fn copy_note_of(device: &Device) -> Option<&str> {
    device
        .copy_note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
}

/// Which rung of `export` this device asks for.
pub fn for_format(device: &Device) -> &'static str {
    if device.kind == "kindle" {
        "kindle"
    } else {
        "epub"
    }
}

pub fn where_line(device: &Device) -> String {
    match &device.volume {
        Some(v) if !v.trim().is_empty() => v.clone(),
        _ => "reached over USB, without mounting as a drive".into(),
    }
}

/// Why nothing can be sent for this script yet.
pub fn blocked_reason(script: Option<&Script>) -> Option<&'static str> {
    let Some(script) = script else {
        return Some("Convert a script first.");
    };
    if script
        .epub_path
        .as_deref()
        .is_none_or(|p| p.trim().is_empty())
    {
        return Some(
            "Method Reader has no book on disk for this script, so there is nothing to copy across. Converting it again puts one in the library.",
        );
    }
    None
}

/// This script's own tuning, as the engine takes it; never the defaults.
pub fn options_json_for(script: &Script) -> Option<String> {
    let settings = script.settings.as_ref()?;
    if settings.is_empty() {
        return None;
    }
    Some(Value::Object(settings.clone()).to_string())
}

/// What the surface may claim once the engine says a copy is done.
pub fn sent_line(device: &Device, answer: &Value) -> String {
    let destination = str_at(answer, "destination").map(str::trim).unwrap_or("");
    if answer.get("uploaded") == Some(&Value::Bool(true)) || destination.is_empty() {
        format!("Sent to {}.", device.name)
    } else if answer.get("safeToUnplug") == Some(&Value::Bool(true)) {
        // A Kindle over MTP never mounted: nothing to eject.
        format!("Sent to {}: {destination}. {SAFE_TO_UNPLUG}", device.name)
    } else {
        format!(
            "Sent to {}: {destination}. Eject the volume before you unplug it.",
            device.name
        )
    }
}

/// What actually went across, in the engine's own words.
pub fn artifact_line(built: &Value) -> String {
    str_at(built, "label")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| str_at(built, "path").map(str::trim))
        .unwrap_or("")
        .to_string()
}

pub const NO_MESSAGE: &str = "The engine refused to send without saying why.";

/// The status line: its words and whether it is an alarm.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Status {
    pub line: String,
    pub bad: bool,
}

impl Status {
    pub fn good(line: impl Into<String>) -> Self {
        Self {
            line: line.into(),
            bad: false,
        }
    }
    pub fn failed(detail: &str) -> Self {
        let said = detail.trim();
        Self {
            line: if said.is_empty() {
                NO_MESSAGE.into()
            } else {
                said.into()
            },
            bad: true,
        }
    }
}

pub fn preparing(device: &Device) -> Status {
    Status::good(format!("Getting the book ready for {}…", device.name))
}

pub fn copying(device: &Device) -> Status {
    Status::good(match copy_note_of(device) {
        Some(note) => format!("Copying it to {}… {note}", device.name),
        None => format!("Copying it to {}…", device.name),
    })
}

pub fn opening(title: &str) -> Status {
    let t = title.trim();
    Status::good(if t.is_empty() {
        "Opening…".to_string()
    } else {
        format!("Opening {t}…")
    })
}

pub const SAVING: &str = "Saving…";

/// The engine's own sentence for a refusal, or the stand-in.
pub fn failure_message(answer: &Value) -> String {
    error_message(answer).unwrap_or_else(|| NO_MESSAGE.into())
}

/// What an export and a send add up to: Ok(the sent line) or Err(why).
pub fn outcome_for(built: &Value, sent: Option<&Value>, device: &Device) -> Result<String, String> {
    if !ok(built) {
        return Err(failure_message(built));
    }
    let sent = sent.unwrap_or(&Value::Null);
    if !ok(sent) {
        return Err(failure_message(sent));
    }
    Ok(sent_line(device, sent))
}

/// Whether this script's settings still have to be fetched before an export.
pub fn needs_settings(script: Option<&Script>) -> bool {
    let Some(script) = script else { return false };
    script.settings.is_none()
        && script
            .fountain_path
            .as_deref()
            .is_some_and(|f| !f.trim().is_empty())
}

/// What the engine's check says about this script's Kindle file.
#[derive(Clone, Debug, PartialEq)]
pub struct KindleFile {
    pub extension: String,
    /// None when not known (a show after a hide: a save may have rewritten
    /// the book since).
    pub fresh: Option<bool>,
    pub built_by_calibre: bool,
    pub book_date: Option<f64>,
    pub beta: bool,
}

fn bare_extension(extension: &str) -> &str {
    extension.strip_prefix('.').unwrap_or(extension)
}

fn plain_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// `kindleCheckFrom(answer)`.
pub fn kindle_check_from(answer: &Value) -> Option<KindleFile> {
    if !ok(answer) {
        return None;
    }
    let extension = bare_extension(str_at(answer, "extension")?.trim()).to_string();
    if !plain_word(&extension) {
        return None;
    }
    let fresh = answer.get("fresh")?.as_bool()?;
    let built_by_calibre = match str_at(answer, "builtBy")? {
        "calibre" => true,
        // The engine's own AZW3, under the product's new name or the
        // LEGACY one the 0.12.x engine answered with.
        "method-reader" | "screepub" => false,
        _ => return None,
    };
    Some(KindleFile {
        extension,
        fresh: Some(fresh),
        built_by_calibre,
        book_date: answer.get("bookDate").and_then(Value::as_f64),
        beta: answer.get("beta") == Some(&Value::Bool(true)),
    })
}

/// Which phase the wait is in before the copy starts.
pub fn may_build(file: Option<&KindleFile>) -> bool {
    file.and_then(|f| f.fresh) != Some(true)
}

pub fn builds_on_press(device: &Device, file: Option<&KindleFile>) -> bool {
    for_format(device) == "kindle" && may_build(file)
}

/// What one build ahead is an attempt at.
pub fn attempt_key(file: &KindleFile) -> String {
    format!(
        "{}@{}",
        file.extension,
        file.book_date.map(|d| d.to_string()).unwrap_or_default()
    )
}

/// Whether the panel starts building the Kindle file before any press.
pub fn builds_ahead(
    file: Option<&KindleFile>,
    building: bool,
    busy: bool,
    tried_for: Option<&str>,
    for_kindle: bool,
) -> bool {
    let Some(file) = file else { return false };
    if file.fresh != Some(false) || !file.built_by_calibre {
        return false;
    }
    if file.book_date.is_none() || tried_for == Some(attempt_key(file).as_str()) {
        return false;
    }
    for_kindle && !building && !busy
}

/// The line over the moving bar while the Kindle file builds.
pub fn build_line(extension: Option<&str>) -> &'static str {
    match extension {
        Some("kfx") => "Building the Kindle file. This takes about half a minute.",
        Some(e) if !e.trim().is_empty() => "Building the Kindle file. This takes a moment.",
        _ => "Building the Kindle file…",
    }
}

/// Why a listed row cannot fire, in the engine's three words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    Connect,
    Platform,
    Setup,
}

/// One route, rebuilt clean.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub id: String,
    pub key: String,
    pub title: String,
    pub detail: String,
    pub button: String,
    pub available: bool,
    pub unavailable: Option<Unavailable>,
    pub device: Option<Device>,
}

fn route_device_from(value: &Value) -> Option<Device> {
    let d = value.as_object()?;
    let text = |k: &str| {
        d.get(k)
            .filter(|v| is_text(Some(v)))
            .and_then(Value::as_str)
    };
    let volume = match d.get("volume")? {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        _ => return None,
    };
    Some(with_transport(
        Device {
            id: text("id")?.into(),
            kind: text("kind")?.into(),
            name: text("name")?.into(),
            volume,
            ..Default::default()
        },
        value,
    ))
}

/// Whether a row goes through export then send.
pub fn is_device_route_key(key: &str) -> bool {
    key.starts_with("device:") || key == "remarkable"
}

fn route_from(value: &Value) -> Option<Route> {
    let r: &Map<String, Value> = value.as_object()?;
    let text = |k: &str| {
        r.get(k)
            .filter(|v| is_text(Some(v)))
            .and_then(Value::as_str)
    };
    let (id, key, title, detail, button) = (
        text("id")?,
        text("key")?,
        text("title")?,
        text("detail")?,
        text("button")?,
    );
    let available = r.get("available")?.as_bool()?;
    let unavailable = match r.get("unavailable") {
        None => None,
        Some(Value::String(s)) => Some(match s.as_str() {
            "connect" => Unavailable::Connect,
            "platform" => Unavailable::Platform,
            "setup" => Unavailable::Setup,
            _ => return None,
        }),
        Some(_) => return None,
    };
    if available == unavailable.is_some() {
        return None;
    }
    let sends_to_device = available && is_device_route_key(key);
    if r.contains_key("device") != sends_to_device {
        return None;
    }
    let device = if sends_to_device {
        Some(route_device_from(r.get("device")?)?)
    } else {
        None
    };
    Some(Route {
        id: id.into(),
        key: key.into(),
        title: title.into(),
        detail: detail.into(),
        button: button.into(),
        available,
        unavailable,
        device,
    })
}

/// `routes --json` as rows and the chosen id, taken whole or not at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Routes {
    pub routes: Vec<Route>,
    pub chosen: String,
}

pub fn routes_from(answer: &Value) -> Option<Routes> {
    if !ok(answer) {
        return None;
    }
    let list: Vec<Route> = answer
        .get("routes")?
        .as_array()?
        .iter()
        .map(route_from)
        .collect::<Option<_>>()?;
    let ids: std::collections::BTreeSet<&str> = list.iter().map(|r| r.id.as_str()).collect();
    if ids.len() != list.len() {
        return None;
    }
    let chosen = str_at(answer, "chosen")?;
    if !ids.contains(chosen) {
        return None;
    }
    Some(Routes {
        routes: list,
        chosen: chosen.into(),
    })
}

/// Whether a row's button hands over the Kindle file.
pub fn hands_over_kindle_file(route: &Route) -> bool {
    route.key == "save-kindle"
        || route
            .device
            .as_ref()
            .is_some_and(|d| for_format(d) == "kindle")
}

pub fn kindle_file_is_beta(route: &Route, file: Option<&KindleFile>) -> bool {
    file.is_some_and(|f| f.beta) && hands_over_kindle_file(route)
}

const SAVE_FALLBACK: &str = "Screenplay";

/// The save box's default name.
pub fn save_name_for(epub_path: &str, extension: &str) -> String {
    let file = epub_path.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = match file.rfind('.') {
        Some(dot) if dot > 0 => &file[..dot],
        _ => file,
    };
    format!(
        "{}.{}",
        if stem.is_empty() { SAVE_FALLBACK } else { stem },
        bare_extension(extension)
    )
}

/// The save box's one filter: (name, extensions).
pub fn save_filter_for(extension: &str, label: Option<&str>) -> (String, Vec<String>) {
    let bare = bare_extension(extension).to_string();
    let name = label
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| bare.to_uppercase());
    (name, vec![bare])
}

pub const NO_NOTE: &str = "The engine said it worked, without saying what it did.";

/// What a route's answer lets the status line say.
pub fn route_note_from(answer: &Value) -> Result<String, String> {
    if !ok(answer) {
        return Err(failure_message(answer));
    }
    Ok(str_at(answer, "note")
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(NO_NOTE)
        .to_string())
}

pub const EMAIL_SETUP: &str = "kindle-email-setup";
pub const SETUP_BUTTON: &str = "Open Amazon’s page";
pub const SETUP_TITLE: &str = "Amazon’s page";
pub const EMAIL_SETUP_LINE: &str =
    "First time? Amazon needs your sender address approved, or it drops the email without a word.";

pub fn email_setup_hint(route: &Route) -> Option<&'static str> {
    (route.key == "email-to-kindle").then_some(EMAIL_SETUP_LINE)
}

/// Which flow a row's button runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Performer {
    Device,
    SaveEpub,
    SaveKindle,
    Setup,
    Open,
}

pub fn performer_for(route: &Route) -> Performer {
    if is_device_route_key(&route.key) {
        return Performer::Device;
    }
    match route.key.as_str() {
        "save-epub" => Performer::SaveEpub,
        "save-kindle" => Performer::SaveKindle,
        EMAIL_SETUP => Performer::Setup,
        _ => Performer::Open,
    }
}

/// Whether a flow's engine call takes the book's turn.
pub fn reads_the_book(how: Performer) -> bool {
    matches!(
        how,
        Performer::Device | Performer::SaveEpub | Performer::SaveKindle
    )
}

/// What is plugged in, read off the route list.
pub fn connected_devices(shown: Option<&Routes>) -> Option<Vec<Device>> {
    Some(
        shown?
            .routes
            .iter()
            .filter_map(|r| r.device.clone())
            .collect(),
    )
}

/// Whether the list on screen is still the list the engine just described.
pub fn same_routes(a: Option<&Routes>, b: Option<&Routes>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a == b)
}

/// What a row says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteLines {
    pub title: String,
    pub detail: String,
    pub where_: Option<String>,
}

pub fn route_lines(route: &Route) -> RouteLines {
    RouteLines {
        title: route.title.clone(),
        detail: route.detail.clone(),
        where_: route
            .device
            .as_ref()
            .filter(|d| d.volume.as_deref().is_some_and(|v| !v.trim().is_empty()))
            .map(where_line),
    }
}

/// What a row's button says, naming the Kindle file where it hands one over.
pub fn button_label(route: &Route, file: Option<&KindleFile>) -> String {
    let words = route.button.clone();
    let Some(file) = file.filter(|_| hands_over_kindle_file(route)) else {
        return words;
    };
    let kind = format!("(.{})", file.extension);
    match words.strip_suffix('…') {
        Some(stem) => format!("{stem} {kind}…"),
        None => format!("{words} {kind}"),
    }
}

pub const NO_EXTENSION: &str =
    "The engine built the Kindle file without saying what kind of file it is.";

/// What "Save a Kindle file" learns from the export: (extension, label).
pub fn kindle_file_from(built: &Value) -> Result<(String, Option<String>), String> {
    if !ok(built) {
        return Err(failure_message(built));
    }
    let extension =
        bare_extension(str_at(built, "extension").map(str::trim).unwrap_or("")).to_string();
    if !plain_word(&extension) {
        return Err(NO_EXTENSION.into());
    }
    Ok((
        extension,
        str_at(built, "label")
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string),
    ))
}

pub const NO_ROUTES: &str =
    "The engine listed the ways to send this book in a shape this window cannot read.";

pub fn routes_failure(answer: &Value) -> String {
    if ok(answer) {
        NO_ROUTES.into()
    } else {
        failure_message(answer)
    }
}

/// The file name at the end of a path.
pub fn base_name(path: &str) -> String {
    path.split(['/', '\\'])
        .rfind(|p| !p.is_empty())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
pub mod fixtures {
    use serde_json::{Value, json};

    pub fn kindle_usb() -> Value {
        json!({"id": "device:kindle", "key": "device:kindle", "title": "Kindle",
            "detail": "copies over USB", "button": "Copy to Kindle", "available": true,
            "device": {"id": "k1", "kind": "kindle", "name": "Kindle", "volume": "/Volumes/Kindle"}})
    }

    pub fn row(id: &str, available: bool) -> Value {
        let mut r = json!({"id": id, "key": id, "title": id, "detail": "d", "button": "Go",
            "available": available});
        if !available {
            r["unavailable"] = json!("connect");
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn device(kind: &str, volume: Option<&str>) -> Device {
        Device {
            id: "d".into(),
            kind: kind.into(),
            name: "Kindle".into(),
            volume: volume.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn a_route_list_is_taken_whole_or_not_at_all() {
        let good = json!({"ok": true, "chosen": "save-epub",
            "routes": [fixtures::row("save-epub", true), fixtures::kindle_usb()]});
        let shown = routes_from(&good).unwrap();
        assert_eq!(shown.routes.len(), 2);
        assert_eq!(
            shown.routes[1].device.as_ref().unwrap().volume.as_deref(),
            Some("/Volumes/Kindle")
        );
        let mut broken = good.clone();
        broken["routes"][0]["title"] = json!(" ");
        assert!(routes_from(&broken).is_none());
        let mut dup = good.clone();
        dup["routes"][1]["id"] = json!("save-epub");
        assert!(routes_from(&dup).is_none());
        let mut lost = good.clone();
        lost["chosen"] = json!("nope");
        assert!(routes_from(&lost).is_none());
        let mut unexplained = good.clone();
        unexplained["routes"][0]["available"] = json!(false);
        assert!(routes_from(&unexplained).is_none());
        let mut no_device = good.clone();
        no_device["routes"][1]
            .as_object_mut()
            .unwrap()
            .remove("device");
        assert!(routes_from(&no_device).is_none());
        let mut stray_device = good;
        stray_device["routes"][0]["device"] = fixtures::kindle_usb()["device"].clone();
        assert!(routes_from(&stray_device).is_none());
        assert!(routes_from(&json!({"ok": false})).is_none());
    }

    #[test]
    fn a_kind_the_window_is_older_than_is_kept() {
        let mut r = fixtures::kindle_usb();
        r["device"]["kind"] = json!("boox");
        r["key"] = json!("device:boox");
        let shown =
            routes_from(&json!({"ok": true, "chosen": "device:kindle", "routes": [r]})).unwrap();
        assert_eq!(shown.routes[0].device.as_ref().unwrap().kind, "boox");
    }

    #[test]
    fn the_save_box_starts_named_for_the_book() {
        assert_eq!(
            save_name_for("/lib/Field Station.epub", "kfx"),
            "Field Station.kfx"
        );
        assert_eq!(
            save_name_for("C:\\lib\\v1.2\\A.b.epub", ".azw3"),
            "A.b.azw3"
        );
        assert_eq!(save_name_for("/lib/.epub", "epub"), ".epub.epub");
        assert_eq!(save_name_for("", "epub"), "Screenplay.epub");
        assert_eq!(
            save_filter_for("kfx", Some("KFX")),
            ("KFX".into(), vec!["kfx".into()])
        );
        assert_eq!(
            save_filter_for(".mobi", None),
            ("MOBI".into(), vec!["mobi".into()])
        );
    }

    #[test]
    fn a_sent_line_never_invents_a_path() {
        let k = device("kindle", Some("/Volumes/Kindle"));
        assert_eq!(sent_line(&k, &json!({"ok": true})), "Sent to Kindle.");
        assert_eq!(
            sent_line(
                &k,
                &json!({"ok": true, "destination": "/Volumes/Kindle/documents/a.kfx"})
            ),
            "Sent to Kindle: /Volumes/Kindle/documents/a.kfx. Eject the volume before you unplug it."
        );
        assert_eq!(
            sent_line(
                &k,
                &json!({"ok": true, "uploaded": true, "destination": "x"})
            ),
            "Sent to Kindle."
        );
    }

    #[test]
    fn a_copy_happened_only_when_both_answers_say_so() {
        let k = device("kindle", None);
        assert_eq!(
            outcome_for(
                &json!({"ok": false, "error": {"message": "no file"}}),
                None,
                &k
            ),
            Err("no file".into())
        );
        assert_eq!(
            outcome_for(&json!({"ok": true}), Some(&json!({"ok": false})), &k),
            Err(NO_MESSAGE.into())
        );
        assert_eq!(
            outcome_for(&json!({"ok": true}), Some(&json!({"ok": true})), &k),
            Ok("Sent to Kindle.".into())
        );
    }

    #[test]
    fn the_kindle_check_is_read_or_ignored() {
        let f = kindle_check_from(&json!({"ok": true, "extension": ".kfx", "fresh": false,
            "builtBy": "calibre", "bookDate": 5, "beta": false}))
        .unwrap();
        assert_eq!(f.extension, "kfx");
        assert_eq!(f.book_date, Some(5.0));
        assert!(
            kindle_check_from(
                &json!({"ok": true, "extension": "k/x", "fresh": true, "builtBy": "calibre"})
            )
            .is_none()
        );
        assert!(
            kindle_check_from(
                &json!({"ok": true, "extension": "kfx", "fresh": true, "builtBy": "x"})
            )
            .is_none()
        );
        let undated = kindle_check_from(
            &json!({"ok": true, "extension": "azw3", "fresh": true, "builtBy": "method-reader"}),
        )
        .unwrap();
        assert_eq!(undated.book_date, None);
        assert!(!undated.built_by_calibre);
        // The old engine's word for its own AZW3 means the same.
        let legacy = kindle_check_from(
            &json!({"ok": true, "extension": "azw3", "fresh": true, "builtBy": "screepub"}),
        )
        .unwrap();
        assert!(!legacy.built_by_calibre);
    }

    #[test]
    fn building_ahead_needs_every_condition() {
        let stale = KindleFile {
            extension: "kfx".into(),
            fresh: Some(false),
            built_by_calibre: true,
            book_date: Some(9.0),
            beta: false,
        };
        assert!(builds_ahead(Some(&stale), false, false, None, true));
        assert!(
            !builds_ahead(Some(&stale), false, false, None, false),
            "a Kobo session"
        );
        assert!(
            !builds_ahead(Some(&stale), true, false, None, true),
            "one is running"
        );
        assert!(
            !builds_ahead(Some(&stale), false, true, None, true),
            "a send is"
        );
        assert!(
            !builds_ahead(Some(&stale), false, false, Some("kfx@9"), true),
            "tried once"
        );
        let ours = KindleFile {
            built_by_calibre: false,
            ..stale.clone()
        };
        assert!(!builds_ahead(Some(&ours), false, false, None, true));
        let unknown = KindleFile {
            fresh: None,
            ..stale.clone()
        };
        assert!(!builds_ahead(Some(&unknown), false, false, None, true));
        let undated = KindleFile {
            book_date: None,
            ..stale
        };
        assert!(!builds_ahead(Some(&undated), false, false, None, true));
        assert!(!builds_ahead(None, false, false, None, true));
    }

    #[test]
    fn the_build_line_names_the_wait() {
        assert_eq!(
            build_line(Some("kfx")),
            "Building the Kindle file. This takes about half a minute."
        );
        assert_eq!(
            build_line(Some("azw3")),
            "Building the Kindle file. This takes a moment."
        );
        assert_eq!(build_line(None), "Building the Kindle file…");
    }

    #[test]
    fn the_kindle_file_is_named_on_the_buttons_that_hand_one_over() {
        let usb = route_from(&fixtures::kindle_usb()).unwrap();
        let file = kindle_check_from(
            &json!({"ok": true, "extension": "kfx", "fresh": true, "builtBy": "calibre"}),
        )
        .unwrap();
        assert_eq!(button_label(&usb, Some(&file)), "Copy to Kindle (.kfx)");
        let mut save = route_from(&fixtures::row("save-kindle", true)).unwrap();
        save.button = "Save a Kindle file…".into();
        assert_eq!(
            button_label(&save, Some(&file)),
            "Save a Kindle file (.kfx)…"
        );
        let epub = route_from(&fixtures::row("save-epub", true)).unwrap();
        assert_eq!(button_label(&epub, Some(&file)), "Go");
        assert_eq!(button_label(&usb, None), "Copy to Kindle");
    }

    #[test]
    fn each_row_runs_its_own_flow() {
        let r = |k: &str| route_from(&fixtures::row(k, false)).unwrap();
        assert_eq!(
            performer_for(&route_from(&fixtures::kindle_usb()).unwrap()),
            Performer::Device
        );
        assert_eq!(performer_for(&r("remarkable")), Performer::Device);
        assert_eq!(performer_for(&r("save-epub")), Performer::SaveEpub);
        assert_eq!(performer_for(&r("save-kindle")), Performer::SaveKindle);
        assert_eq!(performer_for(&r("kindle-email-setup")), Performer::Setup);
        assert_eq!(performer_for(&r("apple-books")), Performer::Open);
        assert!(reads_the_book(Performer::Device) && !reads_the_book(Performer::Open));
    }

    #[test]
    fn a_route_note_is_the_engines_or_says_exactly_what_is_known() {
        assert_eq!(
            route_note_from(&json!({"ok": true, "note": " Opened Books. "})),
            Ok("Opened Books.".into())
        );
        assert_eq!(route_note_from(&json!({"ok": true})), Ok(NO_NOTE.into()));
        assert_eq!(
            route_note_from(&json!({"ok": false, "note": "x"})),
            Err(NO_MESSAGE.into())
        );
    }

    #[test]
    fn the_email_row_carries_its_first_time_step() {
        let email = route_from(&fixtures::row("email-to-kindle", false)).unwrap();
        assert_eq!(email_setup_hint(&email), Some(EMAIL_SETUP_LINE));
        assert_eq!(
            email_setup_hint(&route_from(&fixtures::row("save-epub", true)).unwrap()),
            None
        );
    }

    #[test]
    fn what_is_plugged_in_is_read_off_the_route_list() {
        let shown = routes_from(&json!({"ok": true, "chosen": "device:kindle",
            "routes": [fixtures::kindle_usb(), fixtures::row("kobo", false)]}))
        .unwrap();
        assert_eq!(connected_devices(Some(&shown)).unwrap().len(), 1);
        assert_eq!(connected_devices(None), None);
        assert!(same_routes(Some(&shown), Some(&shown.clone())));
        assert!(!same_routes(Some(&shown), None));
    }

    #[test]
    fn a_kindle_save_learns_its_extension_or_opens_no_box() {
        assert_eq!(
            kindle_file_from(
                &json!({"ok": true, "extension": ".azw3", "label": "AZW3 (Method Reader)"})
            ),
            Ok(("azw3".into(), Some("AZW3 (Method Reader)".into())))
        );
        assert_eq!(
            kindle_file_from(&json!({"ok": true})),
            Err(NO_EXTENSION.into())
        );
        assert_eq!(routes_failure(&json!({"ok": true})), NO_ROUTES);
        assert_eq!(base_name("/a/b.epub"), "b.epub");
    }

    #[test]
    fn a_book_with_no_epub_cannot_be_sent() {
        use crate::model::convert::script_from;
        assert_eq!(blocked_reason(None), Some("Convert a script first."));
        let none = script_from("/a.pdf", &json!({}));
        assert!(
            blocked_reason(Some(&none))
                .unwrap()
                .starts_with("Method Reader has no book")
        );
        let book = script_from(
            "/a.pdf",
            &json!({"epubPath": "/lib/a.epub", "fountainPath": "/lib/a.fountain"}),
        );
        assert_eq!(blocked_reason(Some(&book)), None);
        assert!(needs_settings(Some(&book)));
        assert_eq!(options_json_for(&book), None);
        assert_eq!(undetectable(Platform::Mac), None);
        assert!(undetectable(Platform::Windows).unwrap().contains(UNPROVEN));
    }
}
