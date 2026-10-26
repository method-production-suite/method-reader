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

/// What a CLI run printed, kept for the test to read.
struct Keep(std::sync::Arc<std::sync::Mutex<String>>);

impl sp_cli::Sink for Keep {
    fn out(&mut self, line: &str) {
        let mut said = self.0.lock().unwrap();
        said.push_str(line);
        said.push('\n');
    }
    fn err(&mut self, line: &str) {
        self.out(line);
    }
}

/// The device library's two requests are the window's own (app.js had no
/// device library, so there is nothing frozen to hold them to): each must
/// be the CLI's verb exactly as its `--help` spells it, so the direct
/// engine runs that verb (tests/direct_engine.rs runs both for real).
#[test]
fn the_device_library_requests_are_the_clis_verbs() {
    let books = Request::DeviceBooks {
        volume: "/Volumes/Kindle".into(),
    };
    assert_eq!(books.argv(), ["device-books", "/Volumes/Kindle", "--json"]);
    assert_eq!(books.kind(), "device-books");
    let remove = Request::DeviceRemove {
        volume: "/Volumes/Kindle".into(),
        file: "/Volumes/Kindle/documents/Field Station.azw3".into(),
    };
    assert_eq!(
        remove.argv(),
        [
            "device-remove",
            "/Volumes/Kindle",
            "/Volumes/Kindle/documents/Field Station.azw3",
            "--json"
        ]
    );
    assert_eq!(remove.kind(), "device-remove");
    // Listing is a look, not somebody's work; removing is work.
    use sp_app::model::busy::counts_toward_busy;
    assert!(!counts_toward_busy(books.call()));
    assert!(counts_toward_busy(remove.call()));
    // Each verb's own --help names the argv's shape.
    for (verb, shape) in [
        (
            "device-books",
            "method-reader device-books <volume> [--json]",
        ),
        (
            "device-remove",
            "method-reader device-remove <volume> <book> [--json]",
        ),
    ] {
        let said = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let code = sp_cli::run(
            &[verb.to_string(), "--help".to_string()],
            Box::new(Keep(said.clone())),
        );
        assert_eq!(code, 0, "{verb} --help");
        let said = said.lock().unwrap().clone();
        assert!(said.contains(shape), "{verb} --help: {said}");
    }
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

/// The report sheet's counts: `shape <fountain> --json`, a verb app.js
/// never had, so it is held here to the CLI's own argv and run through the
/// direct engine (src/direct.rs), which answers integers only. The CLI's
/// side is sp-cli/tests/cli_shape.rs.
#[test]
fn the_shape_request_is_the_cli_s_shape_verb() {
    use sp_app::engine::Engine;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/field-station/script.fountain")
        .to_string_lossy()
        .into_owned();
    let request = Request::Shape {
        fountain: path.clone(),
    };
    assert_eq!(request.argv(), ["shape", path.as_str(), "--json"]);
    assert_eq!(request.kind(), "shape");
    let reply = sp_app::direct::DirectEngine::new().run(&request, &mut |_| {});
    let answer = reply.answer.expect("an answer");
    assert_eq!(answer["ok"], true);
    for (key, value) in answer.as_object().unwrap() {
        assert!(key == "ok" || value.is_u64(), "{key} is {value}");
    }
    let shape = sp_app::model::feedback::Shape::from_answer(&answer).expect("a shape");
    assert_eq!(shape.pages, Some(18));
    assert_eq!(shape.scene_headings, 11);
    // The fake answers what the real engine does for the demo script.
    assert_eq!(answer, sp_app::engine::default_answer(&request));
}
