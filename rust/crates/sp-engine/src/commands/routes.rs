//! Port of `src/cli-routes.ts`: the `routes` verb (every way out, best
//! first, the one to choose already chosen) and the `route` verb (perform
//! one that is not a reader over USB). This chooses nothing: it checks the
//! request and carries the catalog's, the performers' and the export's
//! answers out.

use serde::Serialize;
use serde_json::Value;

use crate::commands::export::{ExportDeps, ExportOptions, export_command};
use crate::env::{Env, host_platform};
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::export::renderers::Renderers;
use crate::export::route_facts::{RouteProbes, route_facts};
use crate::export::route_perform::{
    Opener, SizeOf, add_to_apple_books, email_to_kindle, open_kindle_email_settings, real_opener,
    real_size_of, send_via_amazon,
};
use crate::export::routes::{Route, RouteFacts, preselected, routes};
use crate::fsx;
use crate::settings::app::{app_settings_path, read_app_settings, write_app_settings};

/// `RoutesAnswer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RoutesAnswer {
    pub routes: Vec<Route>,
    /// The id (not the key) of the row to choose first.
    pub chosen: String,
}

/// `requireBook(epub)`: a file that is there, before anything is probed.
pub fn require_book(epub: &str) -> Result<(), EngineError> {
    if fsx::is_file(epub) {
        Ok(())
    } else {
        Err(CliError::new(
            ErrorCode::Unreadable,
            format!("cannot read the book: {epub}"),
        )
        .into())
    }
}

fn settings_file(settings_path: Option<&str>) -> String {
    settings_path
        .map(str::to_owned)
        .unwrap_or_else(|| app_settings_path(host_platform(), &Env::process()))
}

/// `routesCommand(epub, deps)`: read-only; never writes the settings file.
/// `facts` is this machine's, gathered by the caller (see route_facts.rs).
pub fn routes_command(
    epub: &str,
    facts: &dyn Fn() -> RouteFacts,
    settings_path: Option<&str>,
) -> Result<RoutesAnswer, EngineError> {
    require_book(epub)?;
    let list = routes(&facts());
    let stored = read_app_settings(&settings_file(settings_path));
    let chosen = preselected(&list, stored.get("lastRoute")).id.clone();
    Ok(RoutesAnswer {
        routes: list,
        chosen,
    })
}

/// `routesLines(answer)`: one line per row for a person.
pub fn routes_lines(answer: &RoutesAnswer) -> Vec<String> {
    let width = answer
        .routes
        .iter()
        .map(|r| r.key.encode_utf16().count())
        .max()
        .unwrap_or(0);
    answer
        .routes
        .iter()
        .map(|r| {
            let mark = if r.id == answer.chosen { '*' } else { ' ' };
            let detail = if r.available {
                r.detail.clone()
            } else {
                format!("(dimmed: {})", r.detail)
            };
            let pad = width.saturating_sub(r.key.encode_utf16().count());
            format!("{mark} {}{}  {}: {detail}", r.key, " ".repeat(pad), r.title)
        })
        .collect()
}

/// The routes this verb opens a program for, and the name each is refused
/// by when its row is not listed at all.
const PERFORMED: [(&str, &str); 3] = [
    ("apple-books", "Apple Books"),
    ("send-to-kindle", "Amazon's Send to Kindle"),
    ("email-to-kindle", "Mail"),
];

/// The two routes that write a copy where --out says.
const SAVES: [&str; 2] = ["save-epub", "save-kindle"];

/// The email row's one-time setup.
const EMAIL_SETUP: &str = "kindle-email-setup";

/// `ROUTE_KEYS`: every key `route` takes, in --help's order.
pub const ROUTE_KEYS: [&str; 6] = [
    "apple-books",
    "send-to-kindle",
    "email-to-kindle",
    "save-epub",
    "save-kindle",
    EMAIL_SETUP,
];

/// `RouteDeps`: every seam, each defaulting to the real thing.
#[derive(Default)]
pub struct RouteDeps<'a> {
    /// The facts; default: this machine's, without the device scan.
    pub facts: Option<Box<dyn Fn() -> RouteFacts + 'a>>,
    /// The settings file (lastRoute, and the export's defaults unless
    /// `export_deps` names its own).
    pub settings_path: Option<String>,
    pub open: Option<Box<Opener<'a>>>,
    pub size_of: Option<Box<SizeOf<'a>>>,
    pub export_deps: Option<ExportDeps<'a>>,
    /// kindle-email-setup's platform; default: this machine's.
    pub platform: Option<String>,
}

/// `RouteOptions`.
#[derive(Debug, Clone, Default)]
pub struct RouteOptions {
    pub key: String,
    pub epub: Option<String>,
    pub out: Option<String>,
    pub fountain: Option<String>,
    pub options_json: Option<String>,
}

/// `RouteAnswer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RouteAnswer {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub note: String,
}

/// `rememberRoute(key, settingsPath)`: remember a route that worked. Never
/// fails whatever called it.
pub fn remember_route(key: &str, settings_path: Option<&str>) {
    let _ = write_app_settings(
        &vec![("lastRoute".to_owned(), Some(Value::String(key.to_owned())))],
        &settings_file(settings_path),
    );
}

fn usage(message: impl Into<String>) -> EngineError {
    CliError::new(ErrorCode::Usage, message).into()
}

/// `leadingCapital(text)`.
fn leading_capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `refuseFlags`: every flag this key cannot act on, refused.
fn refuse_flags(
    options: &RouteOptions,
    who: &str,
    out: bool,
    build: bool,
) -> Result<(), EngineError> {
    if !out && options.out.is_some() {
        return Err(usage(format!(
            "{who} takes no --out (only save-epub and save-kindle write a file)"
        )));
    }
    if !build {
        if options.fountain.is_some() {
            return Err(usage(format!(
                "{who} takes no --fountain (only save-kindle builds a file)"
            )));
        }
        if options.options_json.is_some() {
            return Err(usage(format!(
                "{who} takes no --options-json (only save-kindle builds a file)"
            )));
        }
    }
    Ok(())
}

/// `routeCommand(options, deps)`: refuse what the request alone can
/// refuse, then availability (from the same catalog the panel draws), then
/// the work, then, on success only, the memory.
pub fn route_command(
    options: &RouteOptions,
    deps: RouteDeps,
    renderers: &dyn Renderers,
) -> Result<RouteAnswer, EngineError> {
    let key = options.key.as_str();
    let open: &Opener = match &deps.open {
        Some(o) => o.as_ref(),
        None => &real_opener,
    };

    if key.starts_with("device:") || key == "remarkable" {
        return Err(usage(
            "send to a reader with method-reader send; route performs the others",
        ));
    }
    if !ROUTE_KEYS.contains(&key) {
        return Err(usage(format!(
            "no route is called \"{key}\": route takes {}",
            ROUTE_KEYS.join(", ")
        )));
    }

    if key == EMAIL_SETUP {
        if options.epub.is_some() {
            return Err(usage(format!(
                "{EMAIL_SETUP} takes no book: it opens Amazon's settings page"
            )));
        }
        refuse_flags(options, EMAIL_SETUP, false, false)?;
        let platform = deps
            .platform
            .clone()
            .unwrap_or_else(|| host_platform().to_owned());
        let note = open_kindle_email_settings(&platform, open)
            .map_err(|err| CliError::new(ErrorCode::RouteFailed, err.message()))?;
        return Ok(RouteAnswer {
            key: key.into(),
            path: None,
            note,
        });
    }

    let Some(epub) = options.epub.as_deref() else {
        return Err(usage(format!(
            "route {key} needs the book: method-reader route {key} <file.epub>"
        )));
    };
    require_book(epub)?;

    let save = SAVES.contains(&key);
    if save && options.out.is_none() {
        return Err(usage(format!(
            "route {key} needs --out: the absolute path to save the copy to"
        )));
    }
    refuse_flags(options, &format!("route {key}"), save, key == "save-kindle")?;

    if save {
        let mut export_deps = deps.export_deps.unwrap_or_default();
        if export_deps.app_settings_path.is_none() {
            export_deps.app_settings_path = deps.settings_path.clone();
        }
        let result = export_command(
            &ExportOptions {
                epub: epub.into(),
                for_: Some(
                    if key == "save-kindle" {
                        "kindle"
                    } else {
                        "epub"
                    }
                    .into(),
                ),
                fountain: options.fountain.clone(),
                options_json: options.options_json.clone(),
                out: options.out.clone(),
                check: false,
            },
            &export_deps,
            renderers,
        )?;
        remember_route(key, deps.settings_path.as_deref());
        return Ok(RouteAnswer {
            key: key.into(),
            note: format!("Saved to {}.", result.path),
            path: Some(result.path),
        });
    }

    let facts = match &deps.facts {
        Some(f) => f(),
        // Without the device scan: route never performs a device row.
        None => route_facts(
            &RouteProbes {
                devices: Some(Box::new(Vec::new)),
                ..Default::default()
            },
            &sp_device::ListDevicesOptions::default(),
        ),
    };
    let list = routes(&facts);
    let Some(row) = list.iter().find(|r| r.key == key) else {
        let name = PERFORMED
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(key, |(_, n)| *n);
        return Err(CliError::new(
            ErrorCode::RouteUnavailable,
            format!("{name} is not installed on this computer"),
        )
        .into());
    };
    if !row.available {
        return Err(
            CliError::new(ErrorCode::RouteUnavailable, leading_capital(&row.detail)).into(),
        );
    }

    let size_of: &SizeOf = match &deps.size_of {
        Some(s) => s.as_ref(),
        None => &real_size_of,
    };
    let performed = match key {
        "apple-books" => add_to_apple_books(epub, open),
        "send-to-kindle" => send_via_amazon(epub, &facts.platform, facts.send_to_kindle_app, open),
        _ => email_to_kindle(epub, open, size_of),
    };
    let note = performed.map_err(|err| match err {
        EngineError::Cli(cli) => EngineError::Cli(cli),
        other => CliError::new(ErrorCode::RouteFailed, other.message()).into(),
    })?;
    remember_route(key, deps.settings_path.as_deref());
    Ok(RouteAnswer {
        key: key.into(),
        path: None,
        note,
    })
}
