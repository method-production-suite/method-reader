//! Port of `src/export/route-facts.ts`: the probes behind the route
//! catalog, gathered here so routes.rs stays pure. Every probe is
//! injectable; the defaults are the only code here that touches the real
//! machine, and the Books, Send to Kindle and mail probes run only on a
//! Mac.
//!
//! The device scan defaults to sp-device's `list_devices`; the CLI passes
//! its own, carrying its test seams (`METHOD_READER_VOLUME_ROOTS`, ...).

use std::process::Stdio;

use crate::env::{Env, host_platform};
use crate::export::routes::RouteFacts;
use crate::nodepath::posix;
use crate::settings::app::home_folder;
use sp_device::types::ConnectedDevice;

/// `MailtoHandlerResult`: a bundle id; `Some(None)` for no mailto entry at
/// all (an unmodified Mac, so Apple Mail); `None` when the probe could not
/// tell (never treated as Apple Mail).
pub type MailtoHandlerResult = Option<Option<String>>;

/// `CommandResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// `CommandRunner`: runs one argv to completion; `Err` when the program
/// cannot be started.
pub type CommandRunner<'a> = dyn Fn(&[&str]) -> Result<CommandResult, String> + 'a;

/// The device scan: every reader connected now.
pub type DevicesProbe<'a> = dyn Fn() -> Vec<ConnectedDevice> + 'a;

/// `existsSync`.
pub type ExistsProbe<'a> = dyn Fn(&str) -> bool + 'a;

/// `RouteProbes`: every probe, each defaulting to the real machine.
#[derive(Default)]
pub struct RouteProbes<'a> {
    pub devices: Option<Box<DevicesProbe<'a>>>,
    pub exists: Option<Box<ExistsProbe<'a>>>,
    pub mailto_handler: Option<Box<dyn Fn() -> MailtoHandlerResult + 'a>>,
    pub run: Option<Box<CommandRunner<'a>>>,
    pub env: Option<Env>,
    pub platform: Option<String>,
}

const BOOKS_APP_PATHS: [&str; 2] = ["/System/Applications/Books.app", "/Applications/Books.app"];
const SEND_TO_KINDLE_APP: &str = "Send to Kindle.app";

fn send_to_kindle_app_paths(env: &Env) -> Vec<String> {
    let home = home_folder(env);
    vec![
        posix::join(&["/Applications", SEND_TO_KINDLE_APP]),
        posix::join(&[home.as_str(), "Applications", SEND_TO_KINDLE_APP]),
    ]
}

/// The one question asked of Launch Services.
pub const MAILTO_HANDLER_ARGV: [&str; 4] = [
    "defaults",
    "read",
    "com.apple.LaunchServices/com.apple.launchservices.secure",
    "LSHandlers",
];

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// JavaScript's `\s`.
fn js_space(c: char) -> bool {
    c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}')
}

/// The `^\s*=\s*(?:"([^"]*)"|([^;{}]+?))\s*;` match on `rest`.
fn assignment_value(rest: &[char]) -> Option<String> {
    let mut i = 0;
    while i < rest.len() && js_space(rest[i]) {
        i += 1;
    }
    if rest.get(i) != Some(&'=') {
        return None;
    }
    let after_eq = &rest[i + 1..];
    // The quoted alternative first.
    let mut j = 0;
    while j < after_eq.len() && js_space(after_eq[j]) {
        j += 1;
    }
    if after_eq.get(j) == Some(&'"')
        && let Some(close) = after_eq[j + 1..].iter().position(|c| *c == '"')
    {
        let inner: String = after_eq[j + 1..j + 1 + close].iter().collect();
        let mut k = j + 1 + close + 1;
        while k < after_eq.len() && js_space(after_eq[k]) {
            k += 1;
        }
        if after_eq.get(k) == Some(&';') {
            return Some(inner);
        }
    }
    // The bare alternative: at least one character that is not ; { },
    // ending at a ;.
    let stop = after_eq.iter().position(|c| matches!(c, ';' | '{' | '}'))?;
    if after_eq[stop] != ';' || stop == 0 {
        return None;
    }
    let segment: String = after_eq[..stop].iter().collect();
    Some(crate::env::js_trim(&segment).to_owned())
}

/// `topLevelValue(dict, key)`: `key = value;` at the dict's OWN top level
/// (depth 1), never inside a nested block.
fn top_level_value(dict: &[char], key: &str) -> Option<String> {
    let key: Vec<char> = key.chars().collect();
    let mut depth = 0i32;
    for i in 0..dict.len() {
        let ch = dict[i];
        if ch == '{' {
            depth += 1;
            continue;
        }
        if ch == '}' {
            depth -= 1;
            continue;
        }
        if depth != 1 {
            continue;
        }
        if !dict[i..].starts_with(&key) {
            continue;
        }
        if i > 0 && is_ident(dict[i - 1]) {
            continue;
        }
        if let Some(value) = assignment_value(&dict[i + key.len()..]) {
            return Some(value);
        }
    }
    None
}

/// `topLevelDicts(text)`: every `{ ... }` at depth 0.
fn top_level_dicts(text: &[char]) -> Vec<&[char]> {
    let mut dicts = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    for (i, ch) in text.iter().enumerate() {
        if *ch == '{' {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if *ch == '}' {
            depth -= 1;
            if depth == 0
                && let Some(s) = start.take()
            {
                dicts.push(&text[s..=i]);
            }
        }
    }
    dicts
}

/// `parseMailtoHandler(defaultsOutput)`: the mailto dict's own
/// `LSHandlerRoleAll`, or `None`.
pub fn parse_mailto_handler(defaults_output: &str) -> Option<String> {
    let text: Vec<char> = defaults_output.chars().collect();
    for dict in top_level_dicts(&text) {
        let Some(scheme) = top_level_value(dict, "LSHandlerURLScheme") else {
            continue;
        };
        if scheme.to_lowercase() != "mailto" {
            continue;
        }
        return top_level_value(dict, "LSHandlerRoleAll");
    }
    None
}

/// The real runner: the program with this process's environment, both
/// streams read.
pub fn real_run(argv: &[&str]) -> Result<CommandResult, String> {
    let (program, args) = argv.split_first().ok_or("no program")?;
    let out = sp_device::process::quiet_command(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    Ok(CommandResult {
        code: out.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// `mailtoHandlerVia(run)`: exit 0 is parsed; "does not exist" is a Mac
/// nobody changed (no entry); anything else could not tell.
pub fn mailto_handler_via(run: &CommandRunner) -> MailtoHandlerResult {
    let result = run(&MAILTO_HANDLER_ARGV).ok()?;
    if result.code == 0 {
        return Some(parse_mailto_handler(&result.stdout));
    }
    if format!("{}\n{}", result.stderr, result.stdout).contains("does not exist") {
        return Some(None);
    }
    None
}

/// `routeFacts(probes, deviceOptions)`. With no `devices` probe, the list
/// is sp-device's `list_devices(device_options)`. `skip_remarkable` in the
/// options says the list was gathered without looking for a reMarkable,
/// which the facts then say (`remarkableChecked: false`).
pub fn route_facts(
    probes: &RouteProbes,
    device_options: &sp_device::ListDevicesOptions,
) -> RouteFacts {
    let skip_remarkable = device_options.skip_remarkable;
    let platform = probes
        .platform
        .clone()
        .unwrap_or_else(|| host_platform().to_owned());
    let on_mac = platform == "darwin";

    let mut books_app = false;
    let mut send_to_kindle_app = false;
    let mut apple_mail_default = false;

    if on_mac {
        let exists = |p: &str| match &probes.exists {
            Some(f) => f(p),
            None => crate::fsx::exists(p),
        };
        books_app = BOOKS_APP_PATHS.iter().any(|p| exists(p));
        let env = probes.env.clone().unwrap_or_else(Env::process);
        send_to_kindle_app = send_to_kindle_app_paths(&env).iter().any(|p| exists(p));
        let handler = match &probes.mailto_handler {
            Some(f) => f(),
            None => match &probes.run {
                Some(run) => mailto_handler_via(run.as_ref()),
                None => mailto_handler_via(&real_run),
            },
        };
        apple_mail_default = match handler {
            Some(None) => true,
            Some(Some(id)) => id.to_lowercase() == "com.apple.mail",
            None => false,
        };
    }

    let devices = match &probes.devices {
        Some(f) => f(),
        None => sp_device::list_devices(device_options),
    };
    RouteFacts {
        platform,
        devices,
        remarkable_checked: skip_remarkable.then_some(false),
        books_app,
        send_to_kindle_app,
        apple_mail_default,
    }
}
