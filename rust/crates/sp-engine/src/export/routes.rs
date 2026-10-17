//! Port of `src/export/routes.ts`: the route catalog. Every way a finished
//! book can leave Method Reader, best route first, and the rule that what you
//! chose last time wins. The window draws this list and never ranks it.
//!
//! Pure: the facts arrive already probed (route_facts.rs) and the
//! remembered route arrives as a value.

use serde::Serialize;
use serde_json::Value;

use sp_device::types::{ConnectedDevice, DeviceKind, device_id};

/// `Unavailable`: why a listed row cannot fire right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Unavailable {
    /// Plug it in, dock it.
    Connect,
    /// Not made for this system.
    Platform,
    /// A setting would fix it (the default mail app).
    Setup,
}

/// `RouteDevice`: a connected device as a row carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RouteDevice {
    pub id: String,
    pub kind: DeviceKind,
    pub name: String,
    pub volume: Option<String>,
}

/// `Route`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Route {
    pub id: String,
    pub key: String,
    pub title: String,
    /// The mechanism when available; the fix when not.
    pub detail: String,
    pub button: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<Unavailable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<RouteDevice>,
}

/// `RouteFacts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFacts {
    pub platform: String,
    pub devices: Vec<ConnectedDevice>,
    /// `Some(false)` when the reMarkable was not looked for.
    pub remarkable_checked: Option<bool>,
    pub books_app: bool,
    pub send_to_kindle_app: bool,
    pub apple_mail_default: bool,
}

/// `DEVICE_KINDS`: every volume-mounted kind.
pub fn device_kinds() -> Vec<DeviceKind> {
    DeviceKind::ALL
        .into_iter()
        .filter(|k| *k != DeviceKind::Remarkable)
        .collect()
}

const DEVICE_DETAIL: &str = "over USB, offline, nothing leaves this computer";
const PLUG_IN: &str = "plug in over USB to send";
const TOLINO_ON_WINDOWS: &str =
    "cannot be found on Windows: a Windows drive carries no volume name";
const REMARKABLE_TITLE: &str = "reMarkable";
const REMARKABLE_BUTTON: &str = "Upload to reMarkable";
const REMARKABLE_CHECKING: &str = "checking whether one is docked…";
const APPLE_BOOKS_TITLE: &str = "Apple Books";
const APPLE_BOOKS_BUTTON: &str = "Add to Apple Books";
const EMAIL_TITLE: &str = "Send to Kindle email";
const EMAIL_BUTTON: &str = "Send to Kindle email";

/// The name a connected device's row shows: its own, or its kind's when its
/// own is blank.
fn name_of(device: &ConnectedDevice) -> String {
    if crate::env::js_trim(&device.name).is_empty() {
        device.kind.display_name().to_owned()
    } else {
        device.name.clone()
    }
}

fn summarize(device: &ConnectedDevice) -> RouteDevice {
    RouteDevice {
        id: device_id(device),
        kind: device.kind,
        name: name_of(device),
        volume: device.volume.clone(),
    }
}

fn row(key: &str, title: &str, detail: &str, button: &str) -> Route {
    Route {
        id: key.into(),
        key: key.into(),
        title: title.into(),
        detail: detail.into(),
        button: button.into(),
        available: true,
        unavailable: None,
        device: None,
    }
}

fn dimmed(key: &str, title: &str, detail: &str, button: &str, why: Unavailable) -> Route {
    Route {
        available: false,
        unavailable: Some(why),
        ..row(key, title, detail, button)
    }
}

/// `routes(facts)`: available rows in order, then every row that cannot
/// fire right now with its fix. Never empty.
pub fn routes(facts: &RouteFacts) -> Vec<Route> {
    let on_mac = facts.platform == "darwin";
    let mut list = Vec::new();

    let volume_devices: Vec<&ConnectedDevice> = facts
        .devices
        .iter()
        .filter(|d| d.kind != DeviceKind::Remarkable)
        .collect();
    let remarkable = facts
        .devices
        .iter()
        .find(|d| d.kind == DeviceKind::Remarkable);

    for device in &volume_devices {
        let key = format!("device:{}", device.kind.as_str());
        let name = name_of(device);
        let mut r = row(&key, &name, DEVICE_DETAIL, &format!("Copy to {name}"));
        r.id = format!(
            "{key}#{}",
            device.volume.clone().unwrap_or_else(|| device_id(device))
        );
        r.device = Some(summarize(device));
        list.push(r);
    }
    if let Some(remarkable) = remarkable {
        let mut r = row(
            "remarkable",
            REMARKABLE_TITLE,
            "the EPUB, over its USB connection",
            REMARKABLE_BUTTON,
        );
        r.device = Some(summarize(remarkable));
        list.push(r);
    }
    if on_mac && facts.books_app {
        list.push(row(
            "apple-books",
            APPLE_BOOKS_TITLE,
            "syncs to your iPhone and iPad",
            APPLE_BOOKS_BUTTON,
        ));
    }
    let send_to_kindle = if on_mac && facts.send_to_kindle_app {
        "Send to Kindle app"
    } else {
        "Send to Kindle web"
    };
    list.push(row(
        "send-to-kindle",
        send_to_kindle,
        "via Amazon, the best-looking Kindle result",
        send_to_kindle,
    ));
    let can_email = on_mac && facts.apple_mail_default;
    if can_email {
        list.push(row(
            "email-to-kindle",
            EMAIL_TITLE,
            "a Mail message with the book attached",
            EMAIL_BUTTON,
        ));
    }
    list.push(row(
        "save-epub",
        "Save the EPUB",
        "for email, Apple Books and most e-readers",
        "Save the EPUB…",
    ));
    list.push(row(
        "save-kindle",
        "Save a Kindle file",
        "for copying to a Kindle by hand",
        "Save a Kindle file…",
    ));

    for kind in device_kinds() {
        if volume_devices.iter().any(|d| d.kind == kind) {
            continue;
        }
        let name = kind.display_name();
        let cannot_find = kind == DeviceKind::Tolino && facts.platform == "win32";
        list.push(dimmed(
            &format!("device:{}", kind.as_str()),
            name,
            if cannot_find {
                TOLINO_ON_WINDOWS
            } else {
                PLUG_IN
            },
            &format!("Copy to {name}"),
            if cannot_find {
                Unavailable::Platform
            } else {
                Unavailable::Connect
            },
        ));
    }
    if remarkable.is_none() {
        let detail = if facts.remarkable_checked == Some(false) {
            REMARKABLE_CHECKING
        } else {
            "dock over USB to send"
        };
        list.push(dimmed(
            "remarkable",
            REMARKABLE_TITLE,
            detail,
            REMARKABLE_BUTTON,
            Unavailable::Connect,
        ));
    }
    if !on_mac {
        list.push(dimmed(
            "apple-books",
            APPLE_BOOKS_TITLE,
            "on a Mac only",
            APPLE_BOOKS_BUTTON,
            Unavailable::Platform,
        ));
    }
    if !can_email {
        list.push(if on_mac {
            dimmed(
                "email-to-kindle",
                EMAIL_TITLE,
                "needs Apple Mail as the default mail app, the one mail app the attachment survives",
                EMAIL_BUTTON,
                Unavailable::Setup,
            )
        } else {
            dimmed(
                "email-to-kindle",
                EMAIL_TITLE,
                "on a Mac only; save the EPUB and attach it yourself",
                EMAIL_BUTTON,
                Unavailable::Platform,
            )
        });
    }
    list
}

/// `preselected(list, lastRoute)`: the remembered route whenever it is
/// listed and could work here (even while unavailable), else the first
/// available row. `last_route` is whatever the settings file holds; only a
/// string can match a key.
pub fn preselected<'a>(list: &'a [Route], last_route: Option<&Value>) -> &'a Route {
    if let Some(Value::String(last)) = last_route
        && let Some(remembered) = list.iter().find(|r| &r.key == last)
        && remembered.unavailable != Some(Unavailable::Platform)
    {
        return remembered;
    }
    list.iter()
        .find(|r| r.available)
        .or_else(|| list.first())
        .expect("preselected needs at least one route")
}
