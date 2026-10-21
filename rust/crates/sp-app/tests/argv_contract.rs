//! The window's engine contract, held to the Tauri window's own.
//!
//! desktop/ui/app.js built an argv for every engine call. Its builders were
//! run once under bun, on the inputs below, and their answers frozen in
//! tests/data/app-js-argv.json. Each `Request` here must stand for exactly
//! the argv app.js built for the same call, empty strings and JavaScript's
//! truthiness included, because the direct engine runs that argv
//! (src/direct.rs).

use serde_json::{Value, json};
use sp_app::engine::Request;

fn s(v: &Value) -> String {
    v.as_str().expect("a string").to_owned()
}

/// `v` as an optional string: absent or null is None.
fn opt(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::to_owned)
}

/// The Request the window makes for one app.js call.
fn request(name: &str, args: &[Value]) -> Request {
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Null);
    let field = |i: usize, key: &str| arg(i).get(key).cloned();
    match name {
        "version" => Request::Version,
        "convert" => Request::Convert {
            path: s(&arg(0)),
            force: field(1, "force").and_then(|v| v.as_bool()).unwrap_or(false),
            options: opt(field(1, "optionsJson").as_ref()),
        },
        "reconvert" => Request::Reconvert {
            fountain: s(&arg(0)),
            epub: s(&arg(1)),
            options: s(&arg(2)),
        },
        "devices" => Request::Devices {
            quick: field(0, "quick").and_then(|v| v.as_bool()).unwrap_or(false),
        },
        "eject" => Request::Eject { volume: s(&arg(0)) },
        // The panel's and the Settings page's writes (panel.rs, tune.rs).
        "readerDevice" => Request::SetAppSettings {
            json: json!({ "readerDevice": arg(0) }).to_string(),
        },
        "animatePage" => Request::SetAppSettings {
            json: json!({ "animatePage": arg(0) }).to_string(),
        },
        "typewriterSounds" => Request::SetAppSettings {
            json: json!({ "typewriterSounds": arg(0) }).to_string(),
        },
        "send" => Request::Send {
            file: s(&arg(0)),
            device: opt(args.get(1)),
        },
        "settings" => Request::Settings {
            fountain: s(&arg(0)),
            set: opt(args.get(1)),
        },
        "export" => Request::Export {
            epub: s(&arg(0)),
            for_format: s(&field(1, "forFormat").expect("forFormat")),
            fountain: opt(field(1, "fountain").as_ref()),
            options: opt(field(1, "optionsJson").as_ref()),
            out: opt(field(1, "out").as_ref()),
        },
        "kindleCheck" => Request::KindleCheck { epub: s(&arg(0)) },
        "kfxStatus" => Request::KfxStatus,
        "kfxInstall" => Request::KfxInstall,
        "appSettings" => match opt(args.first()) {
            None => Request::AppSettings,
            Some(json) => Request::SetAppSettings { json },
        },
        "reveal" => Request::Reveal { path: s(&arg(0)) },
        "routes" => Request::Routes {
            epub: s(&arg(0)),
            quick: field(1, "quick").and_then(|v| v.as_bool()).unwrap_or(false),
        },
        "route" => Request::Route {
            key: s(&arg(0)),
            epub: s(&arg(1)),
            out: opt(field(2, "out").as_ref()),
            fountain: opt(field(2, "fountain").as_ref()),
            options: opt(field(2, "optionsJson").as_ref()),
        },
        "emailSetup" => Request::EmailSetup,
        other => panic!("app.js has no builder the window maps: {other}"),
    }
}

#[test]
fn every_request_is_the_argv_app_js_built() {
    let frozen: Vec<Value> =
        serde_json::from_str(include_str!("data/app-js-argv.json")).expect("JSON");
    assert!(frozen.len() >= 39);
    let mut seen = std::collections::BTreeSet::new();
    for call in &frozen {
        let name = call["name"].as_str().unwrap();
        let args = call["args"].as_array().unwrap();
        let want: Vec<String> = serde_json::from_value(call["argv"].clone()).unwrap();
        let got = request(name, args).argv();
        assert_eq!(got, want, "argv.{name}({args:?})");
        seen.insert(name);
    }
    // Every builder app.js exported was asked at least once.
    assert_eq!(seen.len(), 19, "{seen:?}");
}

/// Fountain in: a .fountain the window converts (dropped, or picked with
/// Choose a script) is the same Convert request a PDF is, so its argv is
/// the PDF's with the path swapped, flag for flag. app.js never sent one,
/// so there is no frozen answer to hold it to; the CLI's side of the same
/// argv runs in sp-cli/tests/fountain_in.rs
/// (`the_window_s_convert_argv_works_on_a_fountain`).
#[test]
fn a_fountain_converts_with_the_argv_a_pdf_does() {
    for (force, options) in [(false, None), (true, Some(r#"{"cueIndentPct":41}"#))] {
        let request = |path: &str| Request::Convert {
            path: path.into(),
            force,
            options: options.map(str::to_owned),
        };
        let pdf = request("/s/Weather Log.pdf").argv();
        let fountain = request("/s/Weather Log.fountain").argv();
        assert_eq!(fountain[0], "/s/Weather Log.fountain");
        assert_eq!(
            fountain[1..],
            pdf[1..],
            "force {force}, options {options:?}"
        );
    }
    assert_eq!(
        Request::Convert {
            path: "/s/Weather Log.fountain".into(),
            force: false,
            options: None,
        }
        .argv(),
        [
            "/s/Weather Log.fountain",
            "--json",
            "--progress",
            "--preview-inline",
            "--library"
        ]
    );
}
