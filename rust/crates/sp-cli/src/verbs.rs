//! The verbs: dispatch (`resolveCommand`), the one flag table every verb
//! refuses foreign flags from, and each verb's printing. The handlers are
//! sp-engine's and sp-device's; they return results, and this wraps them in
//! the `{ ok, ... }` envelope or a person's lines.

use serde_json::Value;
use sp_device::{ListDevicesOptions, Platform};
use sp_engine::commands::app_settings::{
    AppSettingsDeps, AppSettingsOptions, app_settings_command,
};
use sp_engine::commands::export::{ExportDeps, ExportOptions, export_command};
use sp_engine::commands::kfx::{
    KfxCommandDeps, install_lines, kfx_install_command, kfx_status_command, setup_lines,
};
use sp_engine::commands::routes::{
    RouteDeps, RouteOptions, remember_route, route_command, routes_command, routes_lines,
};
use sp_engine::commands::settings::{SettingsOptions, settings_command};
use sp_engine::env::{Env, host_platform};
use sp_engine::errors::EngineError;
use sp_engine::export::kfx_setup::kfx_possible;
use sp_engine::export::renderers::Books;
use sp_engine::export::route_facts::{RouteProbes, route_facts};
use sp_engine::jsjson;
use sp_engine::nodepath::host;
use sp_engine::settings::app::app_settings_path;
use sp_engine::update::{update_decision_command, update_should_check_command};

use crate::args::{self, Parsed, Spec, flag, short, string};
use crate::out::{Io, Run, Stop};
use crate::usage::*;

/// `VERBS`, in their order.
pub const VERBS: [&str; 13] = [
    "devices",
    "send",
    "settings",
    "export",
    "update-decision",
    "update-should-check",
    "kfx-status",
    "kfx-install",
    "app-settings",
    "reveal",
    "routes",
    "route",
    "eject",
];

/// `resolveCommand(argv)`: the FIRST argument is a verb only when it is one
/// exactly AND no file of that name exists.
pub fn resolve_command(argv: &[String]) -> Option<(&'static str, &[String])> {
    let first = argv.first()?;
    let verb = VERBS.iter().find(|v| **v == first.as_str())?;
    if std::path::Path::new(first).exists() {
        return None;
    }
    Some((verb, &argv[1..]))
}

const VERB_SPECS: &[Spec] = &[
    string("device"),
    string("set"),
    string("for"),
    string("fountain"),
    string("options-json"),
    string("out"),
    string("offered"),
    string("current"),
    flag("opted-in"),
    string("last-checked"),
    flag("quick"),
    flag("check"),
    flag("json"),
    short(flag("help"), 'h'),
];

/// `VERB_FLAGS`: the flags each verb acts on.
fn verb_flags(verb: &str) -> &'static [&'static str] {
    match verb {
        "devices" | "routes" => &["--quick"],
        "send" => &["--device"],
        "settings" | "app-settings" => &["--set"],
        "export" => &["--for", "--fountain", "--options-json", "--out", "--check"],
        "update-decision" => &["--offered", "--current"],
        "update-should-check" => &["--opted-in", "--last-checked"],
        "route" => &["--out", "--fountain", "--options-json"],
        _ => &[],
    }
}

const UPDATE_FLAGS: [&str; 4] = ["--offered", "--current", "--last-checked", "--opted-in"];
const OTHER_FLAGS: [&str; 8] = [
    "--out",
    "--device",
    "--set",
    "--for",
    "--fountain",
    "--options-json",
    "--quick",
    "--check",
];

/// `foreignFlagRefusal(verb, values)`.
fn foreign_flag_refusal(verb: &str, values: &Parsed) -> Option<String> {
    let is_update = verb == "update-decision" || verb == "update-should-check";
    let order: Vec<&str> = if is_update {
        OTHER_FLAGS.iter().chain(&UPDATE_FLAGS).copied().collect()
    } else {
        UPDATE_FLAGS.iter().chain(&OTHER_FLAGS).copied().collect()
    };
    for f in order {
        let name = &f[2..];
        let given = values.string(name).is_some() || values.flag(name);
        if !given || verb_flags(verb).contains(&f) {
            continue;
        }
        let owners: Vec<&str> = VERBS
            .iter()
            .copied()
            .filter(|v| verb_flags(v).contains(&f))
            .collect();
        let aside = if verb == "devices" && f == "--device" {
            ": it lists every reader"
        } else {
            ""
        };
        return Some(format!(
            "{verb} takes no {f}{aside} ({f} belongs to {})",
            owners.join(" and ")
        ));
    }
    None
}

fn verb_usage(verb: &str) -> &'static str {
    match verb {
        "devices" => DEVICES_USAGE,
        "settings" => SETTINGS_USAGE,
        "export" => EXPORT_USAGE,
        "kfx-status" => KFX_STATUS_USAGE,
        "kfx-install" => KFX_INSTALL_USAGE,
        "routes" => ROUTES_USAGE,
        "route" => ROUTE_USAGE,
        "update-decision" => UPDATE_DECISION_USAGE,
        "update-should-check" => UPDATE_SHOULD_CHECK_USAGE,
        "app-settings" => APP_SETTINGS_USAGE,
        "reveal" => REVEAL_USAGE,
        "eject" => EJECT_USAGE,
        _ => SEND_USAGE,
    }
}

/// An engine failure: a contract error as itself, anything else through
/// the catch-all.
fn engine(err: EngineError) -> Stop {
    match err {
        EngineError::Cli(e) => Stop::fail(e.code.as_str(), e.message),
        EngineError::Fs(e) => Stop::Internal(format!("Error: {}", e.message)),
        EngineError::Named { name, message } => Stop::Internal(format!("{name}: {message}")),
    }
}

fn device(err: sp_device::CliError) -> Stop {
    Stop::fail(err.code.as_str(), err.message)
}

fn value<T: serde::Serialize>(answer: &T) -> Value {
    serde_json::to_value(answer).expect("an answer serializes")
}

/// `deviceSeams()`: METHOD_READER_VOLUME_ROOTS and METHOD_READER_REMARKABLE_ENDPOINT.
fn device_seams(skip_remarkable: bool) -> ListDevicesOptions<'static> {
    ListDevicesOptions {
        skip_remarkable,
        ..ListDevicesOptions::from_env()
    }
}

fn settings_file() -> String {
    app_settings_path(host_platform(), &Env::process())
}

/// macOS's camera-import pause for a copy to an MTP Kindle (sp-device),
/// its marker in the folder the app settings file is in.
pub fn camera_import() -> sp_device::camera_import::CameraImportDeps {
    let settings = settings_file();
    let folder = std::path::Path::new(&settings)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    sp_device::camera_import::CameraImportDeps::host(folder)
}

fn no_arguments(verb: &str, positionals: &[String]) -> Run {
    match positionals.first() {
        Some(p) => Err(Stop::usage(format!(
            "{verb} takes no arguments (got \"{p}\")"
        ))),
        None => Ok(()),
    }
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

/// JavaScript's `String(value)` for a settings value.
fn js_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n
            .as_f64()
            .map_or_else(|| n.to_string(), jsjson::number_to_string),
        Value::Null => "null".into(),
        other => jsjson::stringify(other),
    }
}

pub fn run_verb(io: &mut Io, verb: &'static str, rest: &[String]) -> Run {
    let parsed = args::parse(rest, VERB_SPECS).map_err(Stop::usage)?;
    io.json = parsed.flag("json");
    if parsed.flag("help") {
        return Err(io.help(verb_usage(verb)));
    }
    if let Some(refusal) = foreign_flag_refusal(verb, &parsed) {
        return Err(Stop::usage(refusal));
    }
    let pos = &parsed.positionals;
    match verb {
        "devices" => {
            if let Some(p) = pos.first() {
                return Err(Stop::usage(format!(
                    "devices takes no arguments (got \"{p}\")"
                )));
            }
            let result = sp_device::commands::devices_command(
                &device_seams(parsed.flag("quick")),
                Platform::host(),
            );
            if io.json {
                io.ok(value(&result));
            } else if result.devices.is_empty() {
                io.say("no devices connected");
            } else {
                for d in &result.devices {
                    io.say(&format!("{} ({}): {}", d.name, d.kind.as_str(), d.id));
                }
            }
            Ok(())
        }
        "update-decision" => {
            no_arguments(verb, pos)?;
            let decision =
                update_decision_command(parsed.string("offered"), parsed.string("current"))
                    .map_err(Stop::usage)?;
            if io.json {
                io.ok(value(&decision));
            } else {
                io.say(&match &decision {
                    sp_engine::update::UpdateDecision::Offer { version, .. } => {
                        format!("offer {version}")
                    }
                    sp_engine::update::UpdateDecision::Refuse { reason, .. } => {
                        format!("no: {reason}")
                    }
                });
            }
            Ok(())
        }
        "update-should-check" => {
            no_arguments(verb, pos)?;
            let answer = update_should_check_command(
                parsed.flag("opted-in"),
                parsed.string("last-checked"),
                now_ms(),
            )
            .map_err(Stop::usage)?;
            if io.json {
                io.ok(value(&answer));
            } else {
                io.say(if answer.check {
                    "check"
                } else {
                    "do not check"
                });
            }
            Ok(())
        }
        "kfx-status" => {
            no_arguments(verb, pos)?;
            let setup = kfx_status_command(&KfxCommandDeps::default());
            if io.json {
                io.ok(value(&setup));
            } else {
                for line in setup_lines(&setup) {
                    io.say(&line);
                }
            }
            Ok(())
        }
        "kfx-install" => {
            no_arguments(verb, pos)?;
            if !io.json && kfx_possible(host_platform()) {
                io.err("installing the KFX plugin from Calibre's plugin index...");
            }
            let installed = kfx_install_command(&KfxCommandDeps::default()).map_err(engine)?;
            if io.json {
                io.ok(value(&installed));
            } else {
                for line in install_lines(&installed) {
                    io.say(&line);
                }
            }
            Ok(())
        }
        "settings" => {
            if pos.len() != 1 {
                return Err(Stop::usage("expected exactly one .fountain (see --help)"));
            }
            let result = settings_command(&SettingsOptions {
                fountain: pos[0].clone(),
                set: parsed.string("set").map(str::to_owned),
                app_settings_path: settings_file(),
            })
            .map_err(engine)?;
            if io.json {
                io.ok(value(&result));
            } else {
                io.say(&format!(
                    "settings for {}: {}",
                    host::basename(&pos[0]),
                    result.sidecar
                ));
                if let Value::Object(m) = value(&result.settings) {
                    for (k, v) in &m {
                        io.say(&format!("  {k}: {}", js_display(v)));
                    }
                }
            }
            Ok(())
        }
        "export" => {
            if pos.len() != 1 {
                return Err(Stop::usage(
                    "expected exactly one .epub to export (see --help)",
                ));
            }
            let result = export_command(
                &ExportOptions {
                    epub: pos[0].clone(),
                    for_: parsed.string("for").map(str::to_owned),
                    fountain: parsed.string("fountain").map(str::to_owned),
                    options_json: parsed.string("options-json").map(str::to_owned),
                    out: parsed.string("out").map(str::to_owned),
                    check: parsed.flag("check"),
                },
                &ExportDeps::default(),
                &Books,
            )
            .map_err(engine)?;
            if io.json {
                io.ok(result.to_json());
            } else {
                for stage in &result.stages {
                    io.say(&format!("  {stage}"));
                }
                io.say(&format!("{}\n  {}", result.label, result.path));
                if let Some(fresh) = result.fresh {
                    io.say(if fresh {
                        "  built, and current"
                    } else {
                        "  not built yet, or older than the book"
                    });
                }
            }
            Ok(())
        }
        "app-settings" => {
            no_arguments(verb, pos)?;
            let result = app_settings_command(
                &AppSettingsOptions {
                    set: parsed.string("set").map(str::to_owned),
                },
                &AppSettingsDeps::default(),
            )
            .map_err(engine)?;
            if io.json {
                io.ok(value(&result));
            } else {
                let where_ = if result.library.from_env {
                    "set by METHOD_READER_LIBRARY"
                } else if result.library.chosen.is_some() {
                    "the folder you chose"
                } else {
                    "the default folder"
                };
                io.say(&format!(
                    "books are saved in {} ({where_})",
                    result.library.path
                ));
                io.say(if result.customized {
                    "new scripts start from: your own defaults"
                } else {
                    "new scripts start from: Method Reader's defaults"
                });
                io.say(if result.keep_script_settings {
                    "when a PDF is converted: keep its settings"
                } else {
                    "when a PDF is converted: follow the defaults"
                });
            }
            Ok(())
        }
        "reveal" => {
            if pos.len() != 1 {
                return Err(Stop::usage(
                    "expected exactly one file to reveal (see --help)",
                ));
            }
            let result = sp_device::reveal::reveal_here(&pos[0]).map_err(device)?;
            if io.json {
                io.ok(value(&result));
            }
            Ok(())
        }
        "eject" => {
            if pos.len() != 1 {
                return Err(Stop::usage(
                    "expected exactly one reader volume to eject (see --help)",
                ));
            }
            let list = || sp_device::list_devices(&device_seams(true));
            let deps = sp_device::eject::EjectDeps {
                devices: Some(&list),
                ..Default::default()
            };
            let result = sp_device::eject::eject_command(&pos[0], &deps).map_err(device)?;
            if io.json {
                io.ok(value(&result));
            } else {
                io.say(&format!(
                    "Ejected {}. It is safe to unplug.",
                    result.ejected
                ));
            }
            Ok(())
        }
        "routes" => {
            if pos.len() != 1 {
                return Err(Stop::usage("expected exactly one .epub (see --help)"));
            }
            let options = device_seams(parsed.flag("quick"));
            let facts = || route_facts(&RouteProbes::default(), &options);
            let answer = routes_command(&pos[0], &facts, None).map_err(engine)?;
            if io.json {
                io.ok(value(&answer));
            } else {
                for line in routes_lines(&answer) {
                    io.say(&line);
                }
            }
            Ok(())
        }
        "route" => {
            if pos.is_empty() || pos.len() > 2 {
                return Err(Stop::usage(
                    "expected a route key and one .epub (see --help)",
                ));
            }
            let answer = route_command(
                &RouteOptions {
                    key: pos[0].clone(),
                    epub: pos.get(1).cloned(),
                    out: parsed.string("out").map(str::to_owned),
                    fountain: parsed.string("fountain").map(str::to_owned),
                    options_json: parsed.string("options-json").map(str::to_owned),
                },
                RouteDeps::default(),
                &Books,
            )
            .map_err(engine)?;
            if io.json {
                io.ok(value(&answer));
            } else {
                io.say(&answer.note);
            }
            Ok(())
        }
        _ => send(io, pos, &parsed),
    }
}

fn send(io: &mut Io, pos: &[String], parsed: &Parsed) -> Run {
    if pos.len() != 1 {
        return Err(Stop::usage(
            "expected exactly one file to send (see --help)",
        ));
    }
    let remember = |key: &str| remember_route(key, None);
    let mut options = sp_device::commands::SendOptions::new(pos[0].clone(), &remember);
    options.device_id = parsed.string("device").map(str::to_owned);
    options.list = device_seams(false);
    options.camera_import = Some(camera_import());
    let sent = sp_device::commands::send_command(&options).map_err(device)?;
    if io.json {
        // `destination` is OMITTED for a reMarkable, which has no path;
        // `uploaded` takes its place. Which shape is read off the result.
        let mut m = serde_json::Map::new();
        m.insert("device".into(), value(&sent.device));
        if sent.uploaded == Some(true) {
            m.insert("uploaded".into(), Value::Bool(true));
        } else {
            m.insert(
                "destination".into(),
                sent.destination.clone().map_or(Value::Null, Value::String),
            );
        }
        if let Some(safe) = sent.safe_to_unplug {
            m.insert("safeToUnplug".into(), Value::Bool(safe));
        }
        if let Some(paused) = sent.camera_import_paused {
            m.insert("cameraImportPaused".into(), Value::Bool(paused));
        }
        io.ok(Value::Object(m));
        return Ok(());
    }
    let name = host::basename(&pos[0]);
    io.say(&if sent.uploaded == Some(true) {
        format!("sent {name} to {}", sent.device.name)
    } else {
        format!(
            "sent {name} to {}: {}",
            sent.device.name,
            sent.destination.unwrap_or_default()
        )
    });
    Ok(())
}
