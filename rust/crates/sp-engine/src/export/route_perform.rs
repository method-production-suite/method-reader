//! Port of `src/export/route-perform.ts`: the routes that leave Method Reader
//! through another program (Apple Books, Amazon's Send to Kindle, a Mail
//! message, Amazon's settings page). Every open goes through an injectable
//! [`Opener`], so no test ever launches Books, a browser, Mail or the file
//! manager. Each performer returns the sentence the window shows.

use std::io::Read;
use std::process::Stdio;

use crate::errors::{CliError, EngineError, ErrorCode};
use crate::nodepath::host;

/// How an open went: its exit code and its stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub code: i32,
    pub stderr: String,
}

/// `Opener`: runs one argv and reports how it went. A program that failed
/// or was not there is an answer (a non-zero code), never a panic.
pub type Opener<'a> = dyn Fn(&[String]) -> Opened + 'a;

/// Amazon's web uploader.
pub const SEND_TO_KINDLE_URL: &str = "https://www.amazon.com/sendtokindle";

/// Amazon's Personal Document Settings.
pub const KINDLE_EMAIL_SETTINGS_URL: &str =
    "https://www.amazon.com/hz/mycd/myx#/home/settings/pdoc";

/// `realOpener`: spawns argv[0], waits, and returns its exit code and
/// stderr; a program that is not there is code 127 with the reason.
pub fn real_opener(argv: &[String]) -> Opened {
    let Some((program, args)) = argv.split_first() else {
        return Opened {
            code: 127,
            stderr: "no program".into(),
        };
    };
    let child = sp_device::process::quiet_command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(err) => {
            return Opened {
                code: 127,
                stderr: err.to_string(),
            };
        }
    };
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_end(&mut stderr);
    }
    let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
    Opened {
        code,
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    }
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_owned()).collect()
}

/// `openOrThrow`: open, or fail with a sentence naming what did not open
/// and why.
fn open_or_throw(open: &Opener, parts: &[&str], what: &str) -> Result<(), EngineError> {
    let Opened { code, stderr } = open(&argv(parts));
    if code == 0 {
        return Ok(());
    }
    let why = crate::env::js_trim(&stderr);
    let why = if why.is_empty() {
        format!("it exited with code {code}")
    } else {
        why.to_owned()
    };
    Err(EngineError::plain(format!("could not open {what}: {why}")))
}

/// Each platform's way to open a web page in the default browser.
fn url_argv<'a>(platform: &str, url: &'a str) -> Vec<&'a str> {
    match platform {
        "darwin" => vec!["open", url],
        "win32" => vec!["rundll32", "url.dll,FileProtocolHandler", url],
        _ => vec!["xdg-open", url],
    }
}

/// `addToAppleBooks(epub, open)`.
pub fn add_to_apple_books(epub: &str, open: &Opener) -> Result<String, EngineError> {
    open_or_throw(open, &["open", "-a", "Books", epub], "Apple Books")?;
    Ok("Added to Apple Books. It syncs to your iPhone and iPad when Books uses iCloud.".into())
}

/// `sendViaAmazon(epub, facts, open)`: Amazon's app when installed (a Mac
/// app), else its web page FIRST and the book's folder after.
pub fn send_via_amazon(
    epub: &str,
    platform: &str,
    send_to_kindle_app: bool,
    open: &Opener,
) -> Result<String, EngineError> {
    if platform == "darwin" && send_to_kindle_app {
        open_or_throw(
            open,
            &["open", "-a", "Send to Kindle", epub],
            "Amazon's Send to Kindle app",
        )?;
        return Ok("Opened Amazon's Send to Kindle app with the book.".into());
    }
    open_or_throw(
        open,
        &url_argv(platform, SEND_TO_KINDLE_URL),
        "Amazon's Send to Kindle page",
    )?;
    let folder = host::dirname(epub);
    if platform == "win32" {
        // explorer exits 1 even when it did what was asked: not read.
        let _ = open(&argv(&["explorer", folder.as_str()]));
    } else if platform == "darwin" {
        open_or_throw(open, &["open", "-R", epub], "the book's folder")?;
    } else {
        open_or_throw(open, &["xdg-open", folder.as_str()], "the book's folder")?;
    }
    Ok("Opened Amazon's Send to Kindle page, and the book's folder so you can drag it in.".into())
}

/// `KINDLE_EMAIL_LIMIT`: Amazon's 50 MB, in decimal bytes.
pub const KINDLE_EMAIL_LIMIT: u64 = 50 * 1000 * 1000;

/// `SizeOf`: a file's size in bytes.
pub type SizeOf<'a> = dyn Fn(&str) -> Result<u64, EngineError> + 'a;

/// `realSizeOf`.
pub fn real_size_of(path: &str) -> Result<u64, EngineError> {
    crate::fsx::size(path)
}

/// The refusal for a book over the limit, its size rounded UP to a tenth
/// of a megabyte.
fn too_large_for_email(bytes: u64) -> String {
    let mb = crate::jsjson::number_to_string(bytes.div_ceil(100_000) as f64 / 10.0);
    format!(
        "This book is too large for Kindle email. It is {mb} MB, and Amazon takes up to 50 MB by email and drops anything larger without a word. Send it with the Send to Kindle app or web page instead, which take up to 200 MB."
    )
}

/// `emailToKindle(epub, open, sizeOf)`: a new Mail message with the book
/// attached, refused before Mail opens when the book is over the limit.
pub fn email_to_kindle(epub: &str, open: &Opener, size_of: &SizeOf) -> Result<String, EngineError> {
    let bytes = size_of(epub)?;
    if bytes > KINDLE_EMAIL_LIMIT {
        return Err(CliError::new(ErrorCode::RouteUnavailable, too_large_for_email(bytes)).into());
    }
    open_or_throw(open, &["open", "-a", "Mail", epub], "Mail")?;
    Ok(
        "Opened a Mail message with the book attached. Address it to your Kindle's email address."
            .into(),
    )
}

/// `openKindleEmailSettings(platform, open)`.
pub fn open_kindle_email_settings(platform: &str, open: &Opener) -> Result<String, EngineError> {
    open_or_throw(
        open,
        &url_argv(platform, KINDLE_EMAIL_SETTINGS_URL),
        "Amazon's settings page",
    )?;
    Ok("Opened Amazon's settings page. Find your Kindle's email address there, and add the address you send from to the approved list.".into())
}
