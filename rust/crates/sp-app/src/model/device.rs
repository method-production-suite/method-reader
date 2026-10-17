//! The reader's device and what each one offers (was
//! `desktop/ui/device.js`).

use serde_json::Value;

use super::convert::{Platform, Script};
use super::send_routes::{
    Device, KindleFile, PROVEN_KIND, PROVEN_PLATFORM, Route, Routes, SAFE_TO_UNPLUG, Unavailable,
    button_label, failure_message, is_device_route_key, sent_line, undetectable,
};

pub const QUESTION: &str = "What do you read on?";
pub const LOOKING: &str = "Looking for every way to send it…";
pub const OTHER_WAYS: &str = "Other ways";
pub const DIFFERENT: &str = "Different device";
pub const SEND_IT: &str = "Send it";
pub const SAVE_A_FILE: &str = "Save a file";
pub const SEND_BOOK: &str = "Send";
pub const PLUGGED_IN: &str = "Plugged in";

pub struct Card {
    pub kind: &'static str,
    pub name: &'static str,
    pub sub: Option<&'static str>,
}

/// The cards, in the spec's order.
pub const DEVICES: [Card; 5] = [
    Card {
        kind: "kindle",
        name: "Kindle",
        sub: None,
    },
    Card {
        kind: "apple",
        name: "iPhone, iPad or Mac",
        sub: Some("Apple Books"),
    },
    Card {
        kind: "kobo",
        name: "Kobo",
        sub: None,
    },
    Card {
        kind: "tolino",
        name: "tolino",
        sub: None,
    },
    Card {
        kind: "remarkable",
        name: "reMarkable",
        sub: None,
    },
];
pub const MISSING: Card = Card {
    kind: "missing",
    name: "My device isn’t here",
    sub: None,
};
pub const MISSING_LINE: &str = "Method Reader does not know it yet. Tell us which one and we will look at adding it. Until then, the EPUB works on most readers.";

pub fn kinds() -> impl Iterator<Item = &'static str> {
    DEVICES.iter().map(|d| d.kind)
}

pub fn name_of(kind: &str) -> &'static str {
    DEVICES
        .iter()
        .find(|d| d.kind == kind)
        .map(|d| d.name)
        .unwrap_or("reader")
}

/// `app-settings --json`'s readerDevice, or None for none, unknown or a
/// refusal.
pub fn reader_device_from(answer: &Value) -> Option<&'static str> {
    if !super::json::ok(answer) {
        return None;
    }
    let said = answer.get("readerDevice")?.as_str()?;
    kinds().find(|k| *k == said)
}

/// The first connected reader of `kind`.
pub fn plugged_in<'a>(kind: &str, devices: Option<&'a [Device]>) -> Option<&'a Device> {
    if kind == "apple" {
        return None;
    }
    devices?.iter().find(|d| d.kind == kind)
}

/// Built and code-tested, never run on that hardware here.
pub fn unproven(kind: &str, platform: Platform) -> bool {
    if kind == "apple" {
        return false;
    }
    !(kind == PROVEN_KIND && platform == PROVEN_PLATFORM)
}

/// What the asterisk says, or None when there is none.
pub fn asterisk_note(kind: &str, platform: Platform) -> Option<String> {
    if !unproven(kind, platform) {
        return None;
    }
    let tail = "yet. If you try it, tell us how it went.";
    if kind == PROVEN_KIND {
        let os = match platform {
            Platform::Windows => "Windows",
            Platform::Linux => "Linux",
            _ => "this kind of computer",
        };
        return Some(format!(
            "Built and tested in code, never tried with a real Kindle on {os} {tail}"
        ));
    }
    Some(format!(
        "Built and tested in code, never tried on a real {} {tail}",
        name_of(kind)
    ))
}

/// A card as drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardView {
    pub kind: &'static str,
    pub name: &'static str,
    pub sub: Option<&'static str>,
    pub here: bool,
    pub unproven: bool,
}

pub fn cards_for(platform: Platform, devices: Option<&[Device]>) -> Vec<CardView> {
    DEVICES
        .iter()
        .map(|d| CardView {
            kind: d.kind,
            name: d.name,
            sub: d.sub,
            here: plugged_in(d.kind, devices).is_some(),
            unproven: unproven(d.kind, platform),
        })
        .chain(std::iter::once(CardView {
            kind: MISSING.kind,
            name: MISSING.name,
            sub: None,
            here: false,
            unproven: false,
        }))
        .collect()
}

pub const WEB_LINE: &str = "Amazon’s Send to Kindle opens beside the book’s folder. Drag the book into it: Method Reader cannot upload it for you.";
pub const PLUG_KINDLE: &str = "Or plug the Kindle in over USB and copy it across.";
pub const APPLE_ELSEWHERE: &str = "Then put it in iCloud Drive, or email it to yourself, and open it in Books on your iPhone or iPad.";

pub fn plug_line(kind: &str, platform: Platform) -> String {
    if kind == "tolino"
        && let Some(cannot) = undetectable(platform)
    {
        return cannot;
    }
    if kind == "remarkable" {
        return "Dock it over USB, with its USB web interface switched on, and this becomes Upload to reMarkable.".into();
    }
    format!(
        "Plug your {} in over USB and this becomes a Copy button.",
        name_of(kind)
    )
}

/// The engine's button for Send to Kindle on the web.
pub const WEB_BUTTON: &str = "Send to Kindle web";

/// One action: a route and what its button says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Act {
    pub route: Route,
    pub label: String,
}

/// The calm screen for a device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub device: String,
    pub unproven: bool,
    pub brass: Option<Act>,
    pub next: Vec<Act>,
    pub lines: Vec<String>,
    pub waiting: bool,
}

pub fn offer_for(
    device: &str,
    platform: Platform,
    shown: Option<&Routes>,
    kindle: Option<&KindleFile>,
) -> Option<Offer> {
    if !kinds().any(|k| k == device) {
        return None;
    }
    let base = Offer {
        device: device.into(),
        unproven: unproven(device, platform),
        brass: None,
        next: vec![],
        lines: vec![],
        waiting: false,
    };
    let Some(shown) = shown else {
        return Some(Offer {
            waiting: true,
            lines: vec![LOOKING.into()],
            ..base
        });
    };
    let available = |test: &dyn Fn(&Route) -> bool| {
        shown
            .routes
            .iter()
            .find(|r| r.available && test(r))
            .cloned()
    };
    let keyed = |key: &'static str| move |r: &Route| r.key == key;
    let reader_of = |kind: &'static str| {
        move |r: &Route| {
            is_device_route_key(&r.key) && r.device.as_ref().is_some_and(|d| d.kind == kind)
        }
    };
    let act = |route: Option<Route>| {
        route.map(|r| Act {
            label: button_label(&r, kindle),
            route: r,
        })
    };
    let save = act(available(&keyed("save-epub")));
    match device {
        "kindle" => {
            let usb = available(&reader_of("kindle"));
            let web = act(available(&keyed("send-to-kindle")));
            let mail = act(available(&keyed("email-to-kindle")));
            if usb.is_some() {
                return Some(Offer {
                    brass: act(usb),
                    next: [web, mail].into_iter().flatten().collect(),
                    ..base
                });
            }
            let drag = web.as_ref().is_some_and(|w| w.route.button == WEB_BUTTON);
            let brass = web.or_else(|| mail.clone()).or(save);
            let next = mail
                .into_iter()
                .filter(|m| Some(m) != brass.as_ref())
                .collect();
            let lines = if drag {
                vec![WEB_LINE.into(), PLUG_KINDLE.into()]
            } else {
                vec![PLUG_KINDLE.into()]
            };
            Some(Offer {
                brass,
                next,
                lines,
                ..base
            })
        }
        "apple" => {
            let books = if platform == Platform::Mac {
                act(available(&keyed("apple-books")))
            } else {
                None
            };
            if books.is_some() {
                return Some(Offer {
                    brass: books,
                    ..base
                });
            }
            Some(Offer {
                brass: save,
                lines: vec![APPLE_ELSEWHERE.into()],
                ..base
            })
        }
        other => {
            let kind: &'static str = kinds().find(|k| *k == other).unwrap_or("kobo");
            let usb = act(available(&reader_of(kind)));
            if usb.is_some() {
                return Some(Offer { brass: usb, ..base });
            }
            Some(Offer {
                brass: save,
                lines: vec![plug_line(other, platform)],
                ..base
            })
        }
    }
}

/// The reader kind a route row is about, connected or dimmed.
pub fn kind_of_route(route: &Route) -> Option<String> {
    if let Some(d) = &route.device
        && !d.kind.trim().is_empty()
    {
        return Some(d.kind.clone());
    }
    if let Some(kind) = route.key.strip_prefix("device:") {
        return Some(kind.into());
    }
    (route.key == "remarkable").then(|| "remarkable".into())
}

/// Other ways' "Send it": the offer's follow-ups first, then every other
/// route in the engine's order; never the brass one or the two saves.
pub fn send_it_rows(shown: Option<&Routes>, offer: Option<&Offer>) -> Vec<Route> {
    let Some(shown) = shown else { return vec![] };
    let rows: Vec<&Route> = shown
        .routes
        .iter()
        .filter(|r| r.key != "save-epub" && r.key != "save-kindle")
        .collect();
    let lead: Vec<&str> = offer
        .map(|o| o.next.iter().map(|a| a.route.id.as_str()).collect())
        .unwrap_or_default();
    let brass = offer
        .and_then(|o| o.brass.as_ref())
        .map(|a| a.route.id.as_str());
    let mut out: Vec<Route> = lead
        .iter()
        .filter_map(|id| rows.iter().find(|r| r.id == *id).map(|r| (*r).clone()))
        .collect();
    out.extend(
        rows.iter()
            .filter(|r| Some(r.id.as_str()) != brass && !lead.contains(&r.id.as_str()))
            .map(|r| (*r).clone()),
    );
    out
}

fn unplugged_verb(route: &Route) -> Option<&'static str> {
    if route.available
        || route.unavailable != Some(Unavailable::Connect)
        || kind_of_route(route).is_none()
    {
        return None;
    }
    match route.detail.as_str() {
        "plug in over USB to send" => Some("plug in"),
        "dock over USB to send" => Some("dock"),
        _ => None,
    }
}

fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The readers that only need plugging in, folded into one line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fold {
    pub kinds: Vec<String>,
    pub names: Vec<String>,
    pub tail: String,
    pub line: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WayItem {
    Row(Route),
    Fold(Fold),
}

pub fn fold_unplugged(rows: &[Route]) -> Vec<WayItem> {
    let folding: Vec<&Route> = rows
        .iter()
        .filter(|r| unplugged_verb(r).is_some())
        .collect();
    if folding.len() < 2 {
        return rows.iter().cloned().map(WayItem::Row).collect();
    }
    let mut verbs: Vec<&str> = Vec::new();
    for r in &folding {
        let v = unplugged_verb(r).expect("filtered");
        if !verbs.contains(&v) {
            verbs.push(v);
        }
    }
    let names: Vec<String> = folding.iter().map(|r| r.title.clone()).collect();
    let tail = format!(": {} over USB to send.", verbs.join(" or "));
    let fold = Fold {
        kinds: folding.iter().filter_map(|r| kind_of_route(r)).collect(),
        line: format!("{}{tail}", listed(&names)),
        names,
        tail,
    };
    let first = folding[0].id.clone();
    let mut items = Vec::new();
    for r in rows {
        if unplugged_verb(r).is_none() {
            items.push(WayItem::Row(r.clone()));
        } else if r.id == first {
            items.push(WayItem::Fold(fold.clone()));
        }
    }
    items
}

pub struct SaveType {
    pub kind: &'static str,
    pub label: &'static str,
    pub who: &'static str,
    pub beta: bool,
}

pub const SAVE_TYPES: [SaveType; 6] = [
    SaveType {
        kind: "epub",
        label: "EPUB",
        who: "Most readers, Apple Books, and email.",
        beta: false,
    },
    SaveType {
        kind: "kfx",
        label: "KFX",
        who: "Modern Kindles over USB, with the best layout.",
        beta: false,
    },
    SaveType {
        kind: "azw3",
        label: "AZW3",
        who: "Kindles over USB, made by Method Reader with nothing else installed.",
        beta: true,
    },
    SaveType {
        kind: "mobi",
        label: "MOBI",
        who: "The oldest Kindles.",
        beta: false,
    },
    SaveType {
        kind: "fountain",
        label: "Fountain",
        who: "The script as plain text, for screenwriting apps.",
        beta: false,
    },
    SaveType {
        kind: "fdx",
        label: "FDX",
        who: "The script for Final Draft and most screenwriting apps, to edit.",
        beta: true,
    },
];
pub const NEEDS_TOOLS: &str = "This computer cannot make one yet.";
pub const SET_IT_UP: &str = "Set it up";
pub const NO_FOUNTAIN: &str =
    "This book was made without a .fountain, so there is none to work from.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRow {
    pub kind: &'static str,
    pub label: &'static str,
    pub who: &'static str,
    pub beta: bool,
    pub available: bool,
    pub fix: Option<&'static str>,
    pub setup: bool,
}

pub fn save_rows(script: Option<&Script>, kindle: Option<&KindleFile>) -> Vec<SaveRow> {
    let fountain = script
        .and_then(|s| s.fountain_path.as_deref())
        .is_some_and(|f| !f.trim().is_empty());
    SAVE_TYPES
        .iter()
        .map(|t| {
            let row = |available, fix, setup| SaveRow {
                kind: t.kind,
                label: t.label,
                who: t.who,
                beta: t.beta,
                available,
                fix,
                setup,
            };
            if matches!(t.kind, "mobi" | "fountain" | "fdx") && !fountain {
                return row(false, Some(NO_FOUNTAIN), false);
            }
            if t.kind == "kfx" && kindle.is_some_and(|k| k.extension != "kfx") {
                return row(false, Some(NEEDS_TOOLS), true);
            }
            row(true, None, false)
        })
        .collect()
}

/// The book-queue turn for a save of `kind`.
pub fn save_turn(kind: &str) -> &'static str {
    if matches!(kind, "kfx" | "azw3" | "mobi") {
        "kindle"
    } else {
        "copy"
    }
}

/// A reader to eject, with the line to say beside the next book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eject {
    pub id: String,
    pub name: String,
    pub volume: String,
    pub line: String,
}

/// What the panel says after a copy the engine called done.
pub fn copied_state(title: &str, device: &Device, sent: &Value) -> (String, Option<Eject>) {
    let book = if title.trim().is_empty() {
        "The book"
    } else {
        title.trim()
    };
    // A Kindle over MTP: nothing was mounted, so no Eject, and the engine
    // says it can be unplugged now.
    if sent.get("safeToUnplug") == Some(&Value::Bool(true)) {
        return (
            format!("{book} is on your {}. {SAFE_TO_UNPLUG}", device.name),
            None,
        );
    }
    match device.volume.as_deref().filter(|v| !v.trim().is_empty()) {
        None => (sent_line(device, sent), None),
        Some(volume) => {
            let line = format!(
                "{book} is on your {}. Eject it before you unplug.",
                device.name
            );
            (
                line.clone(),
                Some(Eject {
                    id: device.id.clone(),
                    name: device.name.clone(),
                    volume: volume.into(),
                    line,
                }),
            )
        }
    }
}

pub fn eject_label(eject: &Eject) -> String {
    format!("Eject {}", eject.name)
}

/// Eject wears the Beta badge off a Mac.
pub fn eject_is_beta(platform: Platform) -> bool {
    platform != Platform::Mac
}

/// What an eject's answer means: the line, whether it is an alarm, and
/// whether the button stays.
pub fn eject_outcome(answer: &Value, eject: &Eject) -> (String, bool, bool) {
    if super::json::ok(answer) {
        return (
            format!("Your {} is ejected. You can unplug it now.", eject.name),
            false,
            false,
        );
    }
    if answer
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(Value::as_str)
        == Some("unknown-device")
    {
        return (
            format!("Your {} is not plugged in any more.", eject.name),
            false,
            false,
        );
    }
    (failure_message(answer), true, true)
}

pub fn eject_still_there(eject: &Eject, devices: Option<&[Device]>) -> bool {
    devices.is_none_or(|list| {
        list.iter()
            .any(|d| d.volume.as_deref() == Some(eject.volume.as_str()))
    })
}

pub const HEADS_UP_MS: f64 = 8000.0;
pub const HEADS_UP_COPY: &str = "Copy it";
pub const HEADS_UP_LATER: &str = "Not now";

pub fn heads_up_line(title: &str) -> String {
    format!("Kindle plugged in. Copy {title}?")
}

/// Which Kindle, if any, the heads-up offers. `seen` is the Kindle ids of
/// the last list (None before the first, which only seeds).
pub fn heads_up_for(
    seen: Option<&[String]>,
    devices: &[Device],
    open: bool,
    on_result: bool,
) -> (Vec<String>, Option<Device>) {
    let kindles: Vec<&Device> = devices.iter().filter(|d| d.kind == "kindle").collect();
    let ids: Vec<String> = kindles.iter().map(|d| d.id.clone()).collect();
    let Some(seen) = seen else { return (ids, None) };
    let fresh = kindles.into_iter().find(|d| !seen.contains(&d.id)).cloned();
    (ids, if open && !on_result { fresh } else { None })
}

/// Whether this is a Kindle session.
pub fn kindle_session(device: Option<&str>, devices: Option<&[Device]>) -> bool {
    device == Some("kindle") || plugged_in("kindle", devices).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::send_routes::{fixtures, routes_from};
    use serde_json::json;

    fn kindle(id: &str) -> Device {
        Device {
            id: id.into(),
            kind: "kindle".into(),
            name: "Kindle".into(),
            volume: Some("/Volumes/Kindle".into()),
            ..Default::default()
        }
    }

    fn routes(rows: Vec<Value>, chosen: &str) -> Routes {
        routes_from(&json!({"ok": true, "chosen": chosen, "routes": rows})).unwrap()
    }

    #[test]
    fn the_cards_are_the_five_readers_and_the_missing_one() {
        let cards = cards_for(Platform::Mac, Some(&[kindle("k")]));
        let names: Vec<_> = cards.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec![
                "Kindle",
                "iPhone, iPad or Mac",
                "Kobo",
                "tolino",
                "reMarkable",
                "My device isn’t here"
            ]
        );
        assert!(cards[0].here && !cards[0].unproven);
        assert!(cards[2].unproven && !cards[1].unproven);
        assert_eq!(
            reader_device_from(&json!({"ok": true, "readerDevice": "kobo"})),
            Some("kobo")
        );
        assert_eq!(
            reader_device_from(&json!({"ok": true, "readerDevice": "nook"})),
            None
        );
        assert_eq!(
            reader_device_from(&json!({"ok": false, "readerDevice": "kobo"})),
            None
        );
    }

    #[test]
    fn the_asterisk_names_what_nobody_has_tried() {
        assert_eq!(asterisk_note("kindle", Platform::Mac), None);
        assert_eq!(asterisk_note("apple", Platform::Linux), None);
        assert_eq!(
            asterisk_note("kindle", Platform::Windows).unwrap(),
            "Built and tested in code, never tried with a real Kindle on Windows yet. If you try it, tell us how it went."
        );
        assert_eq!(
            asterisk_note("kobo", Platform::Mac).unwrap(),
            "Built and tested in code, never tried on a real Kobo yet. If you try it, tell us how it went."
        );
    }

    #[test]
    fn a_kindle_plugged_in_gets_the_copy_and_the_web_and_email_lead_other_ways() {
        let shown = routes(
            vec![
                fixtures::row("save-epub", true),
                fixtures::kindle_usb(),
                fixtures::row("send-to-kindle", true),
                fixtures::row("email-to-kindle", true),
            ],
            "device:kindle",
        );
        let offer = offer_for("kindle", Platform::Mac, Some(&shown), None).unwrap();
        assert_eq!(offer.brass.as_ref().unwrap().route.id, "device:kindle");
        assert_eq!(offer.next.len(), 2);
        let rows: Vec<_> = send_it_rows(Some(&shown), Some(&offer))
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(rows, vec!["send-to-kindle", "email-to-kindle"]);
    }

    #[test]
    fn a_kindle_not_plugged_in_goes_web_then_email_then_the_epub() {
        let mut web = fixtures::row("send-to-kindle", true);
        web["button"] = json!(WEB_BUTTON);
        let shown = routes(
            vec![
                fixtures::row("save-epub", true),
                web,
                fixtures::row("email-to-kindle", true),
            ],
            "save-epub",
        );
        let offer = offer_for("kindle", Platform::Linux, Some(&shown), None).unwrap();
        assert_eq!(offer.brass.as_ref().unwrap().route.id, "send-to-kindle");
        assert_eq!(
            offer.lines,
            vec![WEB_LINE.to_string(), PLUG_KINDLE.to_string()]
        );
        assert!(offer.unproven);
        let only_save = routes(vec![fixtures::row("save-epub", true)], "save-epub");
        let fallback = offer_for("kindle", Platform::Mac, Some(&only_save), None).unwrap();
        assert_eq!(fallback.brass.unwrap().route.id, "save-epub");
        assert_eq!(fallback.lines, vec![PLUG_KINDLE.to_string()]);
    }

    #[test]
    fn apple_is_books_on_a_mac_and_a_save_elsewhere() {
        let shown = routes(
            vec![
                fixtures::row("save-epub", true),
                fixtures::row("apple-books", true),
            ],
            "apple-books",
        );
        assert_eq!(
            offer_for("apple", Platform::Mac, Some(&shown), None)
                .unwrap()
                .brass
                .unwrap()
                .route
                .id,
            "apple-books"
        );
        let elsewhere = offer_for("apple", Platform::Windows, Some(&shown), None).unwrap();
        assert_eq!(elsewhere.brass.unwrap().route.id, "save-epub");
        assert_eq!(elsewhere.lines, vec![APPLE_ELSEWHERE.to_string()]);
    }

    #[test]
    fn while_the_list_is_out_the_panel_says_it_is_looking() {
        let offer = offer_for("kobo", Platform::Mac, None, None).unwrap();
        assert!(offer.waiting && offer.brass.is_none());
        assert_eq!(offer.lines, vec![LOOKING.to_string()]);
        assert!(offer_for("nook", Platform::Mac, None, None).is_none());
    }

    #[test]
    fn a_tolino_on_windows_is_told_it_cannot_be_found() {
        let shown = routes(vec![fixtures::row("save-epub", true)], "save-epub");
        let offer = offer_for("tolino", Platform::Windows, Some(&shown), None).unwrap();
        assert!(offer.lines[0].starts_with("A tolino cannot be found on Windows"));
        assert_eq!(
            plug_line("kobo", Platform::Mac),
            "Plug your Kobo in over USB and this becomes a Copy button."
        );
        assert!(plug_line("remarkable", Platform::Mac).starts_with("Dock it over USB"));
    }

    fn dimmed(id: &str, title: &str, detail: &str) -> Value {
        json!({"id": id, "key": id, "title": title, "detail": detail, "button": "Copy",
            "available": false, "unavailable": "connect"})
    }

    #[test]
    fn readers_that_only_need_plugging_in_fold_into_one_line() {
        let shown = routes(
            vec![
                fixtures::row("save-epub", true),
                dimmed("device:kobo", "Kobo", "plug in over USB to send"),
                fixtures::row("apple-books", true),
                dimmed("device:tolino", "tolino", "plug in over USB to send"),
                dimmed("remarkable", "reMarkable", "dock over USB to send"),
            ],
            "save-epub",
        );
        let items = fold_unplugged(&send_it_rows(Some(&shown), None));
        assert_eq!(items.len(), 2);
        let WayItem::Fold(fold) = &items[0] else {
            panic!("the fold sits where the first was")
        };
        assert_eq!(
            fold.line,
            "Kobo, tolino and reMarkable: plug in or dock over USB to send."
        );
        assert_eq!(fold.kinds, vec!["kobo", "tolino", "remarkable"]);
        let one = routes(
            vec![dimmed("device:kobo", "Kobo", "plug in over USB to send")],
            "device:kobo",
        );
        assert!(matches!(fold_unplugged(&one.routes)[0], WayItem::Row(_)));
    }

    #[test]
    fn the_save_rows_say_who_each_file_is_for_and_why_one_cannot_be_made() {
        use crate::model::convert::script_from;
        let no_fountain = script_from("/a.pdf", &json!({"epubPath": "/a.epub"}));
        let rows = save_rows(Some(&no_fountain), None);
        assert_eq!(rows.len(), 6);
        for kind in ["mobi", "fountain", "fdx"] {
            let r = rows.iter().find(|r| r.kind == kind).unwrap();
            assert_eq!(r.fix, Some(NO_FOUNTAIN));
        }
        let azw3 = KindleFile {
            extension: "azw3".into(),
            fresh: Some(true),
            built_by_calibre: false,
            book_date: None,
            beta: true,
        };
        let rows = save_rows(Some(&no_fountain), Some(&azw3));
        let kfx = rows.iter().find(|r| r.kind == "kfx").unwrap();
        assert!(!kfx.available && kfx.setup && kfx.fix == Some(NEEDS_TOOLS));
        assert_eq!(save_turn("kfx"), "kindle");
        assert_eq!(save_turn("fdx"), "copy");
        let betas: Vec<_> = SAVE_TYPES
            .iter()
            .filter(|t| t.beta)
            .map(|t| t.kind)
            .collect();
        assert_eq!(betas, vec!["azw3", "fdx"]);
    }

    #[test]
    fn a_copy_to_a_mounted_reader_ends_with_eject() {
        let (line, eject) = copied_state("Field Station", &kindle("k"), &json!({"ok": true}));
        assert_eq!(
            line,
            "Field Station is on your Kindle. Eject it before you unplug."
        );
        let eject = eject.unwrap();
        assert_eq!(eject_label(&eject), "Eject Kindle");
        let tablet = Device {
            id: "r".into(),
            kind: "remarkable".into(),
            name: "reMarkable".into(),
            volume: None,
            ..Default::default()
        };
        let (line, none) = copied_state("x", &tablet, &json!({"ok": true, "uploaded": true}));
        assert_eq!(line, "Sent to reMarkable.");
        assert!(none.is_none());
        assert_eq!(
            eject_outcome(&json!({"ok": true}), &eject).0,
            "Your Kindle is ejected. You can unplug it now."
        );
        assert_eq!(
            eject_outcome(
                &json!({"ok": false, "error": {"code": "unknown-device"}}),
                &eject
            ),
            (
                "Your Kindle is not plugged in any more.".into(),
                false,
                false
            )
        );
        assert_eq!(
            eject_outcome(&json!({"ok": false, "error": {"message": "busy"}}), &eject),
            ("busy".into(), true, true)
        );
        assert!(eject_still_there(&eject, Some(&[kindle("k")])));
        assert!(!eject_still_there(&eject, Some(&[])));
        assert!(eject_still_there(&eject, None));
        assert!(eject_is_beta(Platform::Linux) && !eject_is_beta(Platform::Mac));
    }

    #[test]
    fn the_heads_up_is_once_per_plug_in_and_never_over_the_result() {
        let (seen, show) = heads_up_for(None, &[kindle("k")], true, false);
        assert!(show.is_none(), "the first list only seeds");
        let (seen2, show) = heads_up_for(Some(&seen), &[kindle("k")], true, false);
        assert!(show.is_none(), "already seen");
        let (seen3, show) = heads_up_for(Some(&seen2), &[], true, false);
        assert!(show.is_none());
        let (_, show) = heads_up_for(Some(&seen3), &[kindle("k")], true, false);
        assert_eq!(show.unwrap().id, "k", "plugged in again is news again");
        let (_, over_result) = heads_up_for(Some(&[]), &[kindle("k")], true, true);
        assert!(over_result.is_none());
        let (_, closed) = heads_up_for(Some(&[]), &[kindle("k")], false, false);
        assert!(closed.is_none());
        assert_eq!(
            heads_up_line("Field Station"),
            "Kindle plugged in. Copy Field Station?"
        );
        assert!(kindle_session(Some("kindle"), None));
        assert!(kindle_session(Some("kobo"), Some(&[kindle("k")])));
        assert!(!kindle_session(None, Some(&[])));
    }
}
