//! The window's one door to the engine.
//!
//! What was `desktop/ui/app.js` built an argv for every call and parsed the JSON the
//! engine printed. Here each argv builder is a [`Request`], and `sp-engine`
//! answers it directly: one trait, one method, answers in the CLI's
//! `--json` shape so the window's validators (src/model) stay exactly as
//! strict as they were. `Request::argv` spells the CLI argv each request
//! stands for, which keeps the contract readable in one place and lets the
//! parity harness hold the two faces together.
//!
//! [`FakeEngine`] answers from fixtures for every state a screen can be in,
//! so every screen is testable before sp-engine exists.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Condvar, Mutex};

use serde_json::{Value, json};

use crate::model::busy::Call;

/// Everything the window ever asks the engine, one variant per argv
/// builder in app.js (rust/PARITY.md, rows E1 to E19).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Version,
    Convert {
        path: String,
        force: bool,
        options: Option<String>,
    },
    Reconvert {
        fountain: String,
        epub: String,
        options: String,
    },
    Devices {
        quick: bool,
    },
    Eject {
        volume: String,
    },
    Send {
        file: String,
        device: Option<String>,
    },
    Settings {
        fountain: String,
        set: Option<String>,
    },
    Export {
        epub: String,
        for_format: String,
        fountain: Option<String>,
        options: Option<String>,
        out: Option<String>,
    },
    KindleCheck {
        epub: String,
    },
    KfxStatus,
    KfxInstall,
    AppSettings,
    SetAppSettings {
        json: String,
    },
    Reveal {
        path: String,
    },
    Routes {
        epub: String,
        quick: bool,
    },
    Route {
        key: String,
        epub: String,
        out: Option<String>,
        fountain: Option<String>,
        options: Option<String>,
    },
    EmailSetup,
}

impl Request {
    /// Which call this is, for the busy count.
    pub fn call(&self) -> Call {
        match self {
            Self::Version => Call::Version,
            Self::Convert { .. } => Call::Convert,
            Self::Reconvert { .. } => Call::Reconvert,
            Self::Devices { .. } => Call::Devices,
            Self::Eject { .. } => Call::Eject,
            Self::Send { .. } => Call::Send,
            Self::Settings { .. } => Call::Settings,
            Self::Export { .. } => Call::Export,
            Self::KindleCheck { .. } => Call::KindleCheck,
            Self::KfxStatus => Call::KfxStatus,
            Self::KfxInstall => Call::KfxInstall,
            Self::AppSettings => Call::AppSettingsRead,
            Self::SetAppSettings { .. } => Call::AppSettingsWrite,
            Self::Reveal { .. } => Call::Reveal,
            Self::Routes { .. } => Call::Routes,
            Self::Route { .. } => Call::Route,
            Self::EmailSetup => Call::EmailSetup,
        }
    }

    /// The CLI argv this request stands for, exactly as app.js built it.
    pub fn argv(&self) -> Vec<String> {
        fn v(parts: &[Option<&str>]) -> Vec<String> {
            parts.iter().flatten().map(|s| s.to_string()).collect()
        }
        let flag = |on: bool, f: &'static str| on.then_some(f);
        // app.js's `value ? flag : null, value`, then its null filter: the
        // flag only for a non-empty value, the value whenever it is there,
        // so an empty string rides along without its flag.
        fn truthy(value: &Option<String>, f: &'static str) -> Option<&'static str> {
            value.as_deref().filter(|s| !s.is_empty()).map(|_| f)
        }
        match self {
            Self::Version => v(&[Some("--version"), Some("--json")]),
            Self::Convert {
                path,
                force,
                options,
            } => v(&[
                Some(path),
                Some("--json"),
                Some("--progress"),
                Some("--preview-inline"),
                Some("--library"),
                flag(*force, "--force"),
                truthy(options, "--options-json"),
                options.as_deref(),
            ]),
            Self::Reconvert {
                fountain,
                epub,
                options,
            } => v(&[
                Some(fountain),
                Some("--json"),
                Some("--preview-inline"),
                Some("-o"),
                Some(epub),
                Some("--options-json"),
                Some(options),
            ]),
            Self::Devices { quick } => {
                v(&[Some("devices"), Some("--json"), flag(*quick, "--quick")])
            }
            Self::Eject { volume } => v(&[Some("eject"), Some(volume), Some("--json")]),
            Self::Send { file, device } => v(&[
                Some("send"),
                Some(file),
                Some("--json"),
                truthy(device, "--device"),
                device.as_deref(),
            ]),
            Self::Settings { fountain, set } => v(&[
                Some("settings"),
                Some(fountain),
                Some("--json"),
                truthy(set, "--set"),
                set.as_deref(),
            ]),
            Self::Export {
                epub,
                for_format,
                fountain,
                options,
                out,
            } => v(&[
                Some("export"),
                Some(epub),
                Some("--json"),
                Some("--for"),
                Some(for_format),
                truthy(fountain, "--fountain"),
                fountain.as_deref(),
                truthy(options, "--options-json"),
                options.as_deref(),
                truthy(out, "--out"),
                out.as_deref(),
            ]),
            Self::KindleCheck { epub } => v(&[
                Some("export"),
                Some(epub),
                Some("--json"),
                Some("--for"),
                Some("kindle"),
                Some("--check"),
            ]),
            Self::KfxStatus => v(&[Some("kfx-status"), Some("--json")]),
            Self::KfxInstall => v(&[Some("kfx-install"), Some("--json")]),
            Self::AppSettings => v(&[Some("app-settings"), Some("--json")]),
            Self::SetAppSettings { json } => v(&[
                Some("app-settings"),
                Some("--json"),
                (!json.is_empty()).then_some("--set"),
                Some(json),
            ]),
            Self::Reveal { path } => v(&[Some("reveal"), Some(path), Some("--json")]),
            Self::Routes { epub, quick } => v(&[
                Some("routes"),
                Some(epub),
                Some("--json"),
                flag(*quick, "--quick"),
            ]),
            Self::Route {
                key,
                epub,
                out,
                fountain,
                options,
            } => v(&[
                Some("route"),
                Some(key),
                Some(epub),
                Some("--json"),
                truthy(out, "--out"),
                out.as_deref(),
                truthy(fountain, "--fountain"),
                fountain.as_deref(),
                truthy(options, "--options-json"),
                options.as_deref(),
            ]),
            Self::EmailSetup => v(&[Some("route"), Some("kindle-email-setup"), Some("--json")]),
        }
    }

    /// A short name for the kind of request, for fixtures and tests.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Convert { .. } => "convert",
            Self::Reconvert { .. } => "reconvert",
            Self::Devices { .. } => "devices",
            Self::Eject { .. } => "eject",
            Self::Send { .. } => "send",
            Self::Settings { set: None, .. } => "settings",
            Self::Settings { set: Some(_), .. } => "settings-set",
            Self::Export { .. } => "export",
            Self::KindleCheck { .. } => "kindle-check",
            Self::KfxStatus => "kfx-status",
            Self::KfxInstall => "kfx-install",
            Self::AppSettings => "app-settings",
            Self::SetAppSettings { .. } => "app-settings-set",
            Self::Reveal { .. } => "reveal",
            Self::Routes { .. } => "routes",
            Self::Route { .. } => "route",
            Self::EmailSetup => "email-setup",
        }
    }
}

/// What the engine said. `Err` is a call that could not run at all (the
/// JavaScript's rejected runEngine); its message is fit to show a person.
/// A conversion or a rebuild carries the book's pages in its answer, as
/// `previewHtml` (the CLI's `--preview-inline`).
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub answer: Result<Value, String>,
}

impl Reply {
    pub fn ok(answer: Value) -> Self {
        Self { answer: Ok(answer) }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            answer: Err(message.into()),
        }
    }
}

/// The engine, as the window sees it. `progress` hears each progress line
/// (the `progress` object of `--progress`) as the work goes.
pub trait Engine: Send + Sync {
    fn run(&self, request: &Request, progress: &mut dyn FnMut(Value)) -> Reply;
}

/// The engine's own preview of the invented demo script (tests/fixtures/
/// field-station.pdf), as the TypeScript engine wrote it: the oracle dump.
pub const SAMPLE_PREVIEW: &str = include_str!("../../../oracle/field-station/book.preview.html");

/// The answers a fresh fake engine gives: a clean conversion of an invented
/// script, a default library, no devices, every route the engine lists on a
/// Mac with nothing plugged in.
pub fn default_answer(request: &Request) -> Value {
    let defaults: Value = crate::model::tune::fixtures::defaults();
    match request {
        Request::Version => json!({"ok": true, "version": "0.11.1"}),
        Request::Convert { path, .. } => json!({
            "ok": true, "title": "Field Station", "author": "the Method Reader project", "pages": 19, "scenes": 11,
            "warnings": [], "epubPath": "/library/Field Station/Field Station.epub",
            "fountainPath": "/library/Field Station/Field Station.fountain", "source": path,
            "previewHtml": SAMPLE_PREVIEW,
        }),
        Request::Reconvert { .. } => {
            json!({"ok": true, "warnings": [], "previewHtml": SAMPLE_PREVIEW})
        }
        Request::Devices { .. } => json!({"ok": true, "devices": []}),
        Request::Eject { .. } => json!({"ok": true}),
        Request::Send { .. } => {
            json!({"ok": true, "destination": "/Volumes/Kindle/documents/Field Station.kfx"})
        }
        Request::Settings { set, .. } => {
            let mut settings = defaults.clone();
            if let Some(set) = set
                && let Ok(Value::Object(changes)) = serde_json::from_str::<Value>(set)
            {
                for (k, v) in changes {
                    settings[k] = v;
                }
            }
            json!({
                "ok": true, "settings": settings, "defaults": defaults, "appDefaults": defaults,
                "keepScriptSettings": true, "preset": "kindle",
                "presets": [
                    {"id": "kindle", "displayName": "Kindle e-ink", "settings": defaults},
                    {"id": "phone", "displayName": "Phone", "settings": phone_preset()},
                ],
            })
        }
        Request::Export {
            epub,
            for_format,
            out,
            ..
        } => {
            let ext = if for_format == "kindle" {
                "azw3"
            } else {
                for_format.as_str()
            };
            let path = out
                .clone()
                .unwrap_or_else(|| epub.replace(".epub", &format!(".{ext}")));
            json!({"ok": true, "path": path, "extension": ext, "label": format!("{} (Method Reader)", ext.to_uppercase())})
        }
        Request::KindleCheck { .. } => {
            json!({"ok": true, "extension": "azw3", "fresh": false, "builtBy": "method-reader", "bookDate": 1, "beta": true})
        }
        Request::KfxStatus => json!({"ok": true, "ready": false, "possible": true,
        "summary": "Kindles get AZW3 today. Calibre with its KFX plugin and Kindle Previewer makes KFX, the best layout.",
        "steps": [
            {"id": "calibre", "name": "Calibre", "installed": false, "fix": {"kind": "link", "label": "Get Calibre", "url": "https://calibre-ebook.com/download_osx"}},
            {"id": "previewer", "name": "Kindle Previewer", "installed": false, "fix": {"kind": "link", "label": "Get Kindle Previewer", "url": "https://kdp.amazon.com/en_US/help/topic/G202131170"}},
            {"id": "plugin", "name": "KFX plugin", "installed": false, "fix": {"kind": "after", "why": "Needs Calibre first."}}
        ]}),
        Request::KfxInstall => {
            json!({"ok": false, "error": {"code": "kfx", "message": "Calibre is not installed."}})
        }
        Request::AppSettings | Request::SetAppSettings { .. } => json!({
            "ok": true, "home": "/Users/reader",
            "library": {"path": "/Users/reader/Method Reader", "chosen": null, "platformDefault": "/Users/reader/Method Reader", "fromEnv": false},
            "readerDevice": null, "animatePage": true, "typewriterSounds": false, "panelIntent": "read", "keepScriptSettings": true,
            "formatDefaults": defaults, "shippedDefaults": defaults,
        }),
        Request::Reveal { .. } => json!({"ok": true}),
        Request::Routes { .. } => json!({"ok": true, "chosen": "save-epub", "routes": [
            {"id": "apple-books", "key": "apple-books", "title": "Apple Books", "detail": "Opens the book in Books on this Mac", "button": "Open in Books", "available": true},
            {"id": "send-to-kindle", "key": "send-to-kindle", "title": "Send to Kindle", "detail": "Amazon's Send to Kindle app", "button": "Send to Kindle", "available": true},
            {"id": "email-to-kindle", "key": "email-to-kindle", "title": "Email to Kindle", "detail": "Mail, with the book attached", "button": "Email it", "available": true},
            {"id": "device:kindle", "key": "device:kindle", "title": "Kindle", "detail": "plug in over USB to send", "button": "Copy to Kindle", "available": false, "unavailable": "connect"},
            {"id": "device:kobo", "key": "device:kobo", "title": "Kobo", "detail": "plug in over USB to send", "button": "Copy to Kobo", "available": false, "unavailable": "connect"},
            {"id": "device:tolino", "key": "device:tolino", "title": "tolino", "detail": "plug in over USB to send", "button": "Copy to tolino", "available": false, "unavailable": "connect"},
            {"id": "remarkable", "key": "remarkable", "title": "reMarkable", "detail": "dock over USB to send", "button": "Upload to reMarkable", "available": false, "unavailable": "connect"},
            {"id": "save-epub", "key": "save-epub", "title": "Save the EPUB", "detail": "A copy wherever you choose", "button": "Save the EPUB…", "available": true},
            {"id": "save-kindle", "key": "save-kindle", "title": "Save a Kindle file", "detail": "For a Kindle over USB", "button": "Save a Kindle file…", "available": true}
        ]}),
        Request::Route { key, .. } => json!({"ok": true, "note": format!("Opened {key}.")}),
        Request::EmailSetup => {
            json!({"ok": true, "note": "Opened Amazon’s Personal Document Settings."})
        }
    }
}

fn phone_preset() -> Value {
    let mut p = crate::model::tune::fixtures::defaults();
    p["dialogueSideMarginPct"] = json!(8);
    p["elementSpacingEm"] = json!(0.8);
    p
}

/// A fake engine: answers from fixtures, records every request, and can be
/// told to answer one kind of request differently (once or always).
#[derive(Default)]
pub struct FakeEngine {
    always: Mutex<HashMap<&'static str, Reply>>,
    once: Mutex<HashMap<&'static str, VecDeque<Reply>>>,
    progress: Mutex<Vec<Value>>,
    calls: Mutex<Vec<Request>>,
    /// Kinds whose answer waits for `release` (after any progress lines),
    /// for a test that looks at a screen while a call is out. Only with a
    /// runtime on threads: inline, a held call would wait forever.
    held: Mutex<HashSet<&'static str>>,
    released: Condvar,
}

impl FakeEngine {
    pub fn new() -> Self {
        let fake = Self::default();
        fake.set_progress(vec![
            json!({"stage": "page", "page": 1, "paragraphs": ["EXT. RIDGE ROAD - DAY", "Wind comes down off the ridge in long flat gusts, pushing the grass one way and then, for no reason, the other.", "DELACROIX", "Thirty-one."]}),
            json!({"stage": "parse", "percent": 30, "page": 6, "pages": 18}),
            json!({"stage": "parse", "percent": 60, "page": 18, "pages": 18}),
            json!({"stage": "render", "percent": 100}),
        ]);
        fake
    }

    /// Answer every request of this kind with `reply`.
    pub fn always(&self, kind: &'static str, reply: Reply) {
        self.always.lock().expect("fake").insert(kind, reply);
    }

    /// Answer the next request of this kind with `reply`.
    pub fn once(&self, kind: &'static str, reply: Reply) {
        self.once
            .lock()
            .expect("fake")
            .entry(kind)
            .or_default()
            .push_back(reply);
    }

    /// The progress lines a conversion narrates.
    pub fn set_progress(&self, lines: Vec<Value>) {
        *self.progress.lock().expect("fake") = lines;
    }

    /// Hold the answers of this kind until `release`.
    pub fn hold(&self, kind: &'static str) {
        self.held.lock().expect("fake").insert(kind);
    }

    pub fn release(&self, kind: &'static str) {
        self.held.lock().expect("fake").remove(kind);
        self.released.notify_all();
    }

    /// Every request so far, in order.
    pub fn calls(&self) -> Vec<Request> {
        self.calls.lock().expect("fake").clone()
    }

    pub fn calls_of(&self, kind: &str) -> Vec<Request> {
        self.calls()
            .into_iter()
            .filter(|r| r.kind() == kind)
            .collect()
    }
}

impl Engine for FakeEngine {
    fn run(&self, request: &Request, progress: &mut dyn FnMut(Value)) -> Reply {
        self.calls.lock().expect("fake").push(request.clone());
        if matches!(request, Request::Convert { .. }) {
            for line in self.progress.lock().expect("fake").iter() {
                progress(line.clone());
            }
        }
        let kind = request.kind();
        let mut held = self.held.lock().expect("fake");
        while held.contains(kind) {
            held = self.released.wait(held).expect("fake");
        }
        drop(held);
        if let Some(reply) = self
            .once
            .lock()
            .expect("fake")
            .get_mut(kind)
            .and_then(VecDeque::pop_front)
        {
            return reply;
        }
        if let Some(reply) = self.always.lock().expect("fake").get(kind) {
            return reply.clone();
        }
        Reply::ok(default_answer(request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_argv_is_the_one_app_js_built() {
        let convert = Request::Convert {
            path: "/a.pdf".into(),
            force: true,
            options: Some("{}".into()),
        };
        assert_eq!(
            convert.argv(),
            [
                "/a.pdf",
                "--json",
                "--progress",
                "--preview-inline",
                "--library",
                "--force",
                "--options-json",
                "{}"
            ]
        );
        let plain = Request::Convert {
            path: "/a.pdf".into(),
            force: false,
            options: None,
        };
        assert_eq!(
            plain.argv(),
            [
                "/a.pdf",
                "--json",
                "--progress",
                "--preview-inline",
                "--library"
            ]
        );
        assert_eq!(
            Request::Reconvert {
                fountain: "/a.fountain".into(),
                epub: "/a.epub".into(),
                options: "{}".into()
            }
            .argv(),
            [
                "/a.fountain",
                "--json",
                "--preview-inline",
                "-o",
                "/a.epub",
                "--options-json",
                "{}"
            ]
        );
        assert_eq!(
            Request::Devices { quick: true }.argv(),
            ["devices", "--json", "--quick"]
        );
        assert_eq!(
            Request::Send {
                file: "/a.kfx".into(),
                device: Some("k1".into())
            }
            .argv(),
            ["send", "/a.kfx", "--json", "--device", "k1"]
        );
        assert_eq!(
            Request::Send {
                file: "/a.kfx".into(),
                device: None
            }
            .argv(),
            ["send", "/a.kfx", "--json"]
        );
        assert_eq!(
            Request::Export {
                epub: "/a.epub".into(),
                for_format: "kindle".into(),
                fountain: Some("/a.fountain".into()),
                options: None,
                out: Some("/o.kfx".into())
            }
            .argv(),
            [
                "export",
                "/a.epub",
                "--json",
                "--for",
                "kindle",
                "--fountain",
                "/a.fountain",
                "--out",
                "/o.kfx"
            ]
        );
        assert_eq!(
            Request::KindleCheck {
                epub: "/a.epub".into()
            }
            .argv(),
            ["export", "/a.epub", "--json", "--for", "kindle", "--check"]
        );
        assert_eq!(
            Request::Routes {
                epub: "/a.epub".into(),
                quick: false
            }
            .argv(),
            ["routes", "/a.epub", "--json"]
        );
        assert_eq!(
            Request::EmailSetup.argv(),
            ["route", "kindle-email-setup", "--json"]
        );
        assert_eq!(
            Request::SetAppSettings {
                json: r#"{"readerDevice":"kobo"}"#.into()
            }
            .argv(),
            [
                "app-settings",
                "--json",
                "--set",
                r#"{"readerDevice":"kobo"}"#
            ]
        );
        assert_eq!(
            Request::Route {
                key: "save-epub".into(),
                epub: "/a.epub".into(),
                out: Some("/b.epub".into()),
                fountain: None,
                options: None
            }
            .argv(),
            [
                "route",
                "save-epub",
                "/a.epub",
                "--json",
                "--out",
                "/b.epub"
            ]
        );
    }

    #[test]
    fn the_fake_answers_from_fixtures_and_records_what_it_was_asked() {
        let fake = FakeEngine::new();
        let mut heard = vec![];
        let reply = fake.run(
            &Request::Convert {
                path: "/a.pdf".into(),
                force: false,
                options: None,
            },
            &mut |l| heard.push(l),
        );
        assert_eq!(heard.len(), 4);
        let answer = reply.answer.unwrap();
        assert_eq!(answer["title"], "Field Station");
        assert!(
            answer["previewHtml"]
                .as_str()
                .unwrap()
                .contains("section class=\"scene\"")
        );
        fake.once(
            "convert",
            Reply::ok(json!({"ok": false, "error": {"code": "scanned", "message": "x"}})),
        );
        let refused = fake.run(
            &Request::Convert {
                path: "/b.pdf".into(),
                force: false,
                options: None,
            },
            &mut |_| {},
        );
        assert_eq!(refused.answer.unwrap()["ok"], false);
        assert_eq!(fake.calls_of("convert").len(), 2);
    }

    #[test]
    fn the_fake_settings_validate_and_its_routes_draw() {
        let fake = FakeEngine::new();
        let s = fake.run(
            &Request::Settings {
                fountain: "/a.fountain".into(),
                set: None,
            },
            &mut |_| {},
        );
        let answer = s.answer.unwrap();
        assert!(crate::model::tune::settings_from(&answer).is_some());
        assert_eq!(crate::model::tune::presets_from(&answer).len(), 2);
        let r = fake.run(
            &Request::Routes {
                epub: "/a.epub".into(),
                quick: true,
            },
            &mut |_| {},
        );
        assert!(crate::model::send_routes::routes_from(&r.answer.unwrap()).is_some());
        let k = fake.run(&Request::KfxStatus, &mut |_| {});
        assert!(crate::model::kfx::setup_from(&k.answer.unwrap()).is_some());
        let a = fake.run(&Request::AppSettings, &mut |_| {});
        assert!(crate::model::convert::library_from(&a.answer.unwrap()).is_some());
    }
}
