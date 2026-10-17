//! The device-request service, from the window's side:
//! was `desktop/ui/requests.js`. The window's one network address.

use serde_json::{Value, json};

use super::convert::Platform;

/// The service's suite address, a custom domain on the same Worker. 0.12
/// and earlier post to its workers.dev address, which still answers.
pub const REQUEST_URL: &str = "https://requests.methodreader.com/request";
pub const MAX_DEVICE: usize = 80;
pub const TIMEOUT_MS: u64 = 15_000;

pub const LABEL: &str = "Which device?";
pub const SEND: &str = "Send";
pub const TRY_AGAIN: &str = "Try again";
pub const WHAT_IS_SENT: &str =
    "Sends the device’s name, Method Reader’s version and your platform. Nothing else.";
pub const THANKS: &str = "Thanks. We count every request.";
pub const WORKED: &str = "It worked";
pub const DID_NOT_WORK: &str = "It did not work";

pub const OFFLINE: &str = "Could not send. Check that this computer is online, then try again.";
pub const TOO_MANY: &str =
    "Could not send: too many requests from this computer just now. Try again in a minute.";
pub const GARBLED: &str =
    "Could not send. The request service did not answer properly. Try again later.";

pub fn device_form(name: &str, version: &str, platform: Platform) -> Value {
    json!({"kind": "device", "device": name.trim(), "version": version, "platform": platform.word()})
}

/// The Worker's name for a route nobody here has tried, or None.
pub fn report_route(kind: &str, platform: Platform) -> Option<String> {
    match kind {
        "kobo" | "tolino" | "remarkable" => Some(kind.into()),
        "kindle" => match platform {
            Platform::Windows | Platform::Linux => Some(format!("kindle-{}", platform.word())),
            _ => None,
        },
        _ => None,
    }
}

pub fn report_form(kind: &str, worked: bool, version: &str, platform: Platform) -> Value {
    json!({"kind": "report", "route": report_route(kind, platform), "worked": worked,
        "version": version, "platform": platform.word()})
}

/// What a send's response adds up to: (ok, line). `status` is the HTTP
/// status, `body` the parsed body if it was JSON; a send that never got a
/// response is `None`.
pub fn answer_from(response: Option<(u16, Option<Value>)>) -> (bool, String) {
    let Some((status, body)) = response else {
        return (false, OFFLINE.into());
    };
    if status == 429 {
        return (false, TOO_MANY.into());
    }
    let ok_status = (200..300).contains(&status);
    if ok_status && body.as_ref().and_then(|b| b.get("ok")) == Some(&Value::Bool(true)) {
        return (true, THANKS.into());
    }
    match body
        .as_ref()
        .and_then(|b| b.get("error"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(said) => (false, format!("Could not send. {said}")),
        None => (false, GARBLED.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_forms_carry_the_name_the_version_and_the_platform_and_nothing_else() {
        assert_eq!(
            device_form("  Boox Note ", "0.11.1", Platform::Mac),
            json!({"kind": "device", "device": "Boox Note", "version": "0.11.1", "platform": "mac"})
        );
        assert_eq!(
            report_form("kindle", true, "0.11.1", Platform::Linux),
            json!({"kind": "report", "route": "kindle-linux", "worked": true, "version": "0.11.1", "platform": "linux"})
        );
        assert_eq!(report_route("kindle", Platform::Mac), None);
        assert_eq!(report_route("kobo", Platform::Mac).as_deref(), Some("kobo"));
        assert_eq!(report_route("apple", Platform::Mac), None);
    }

    #[test]
    fn every_answer_reads_as_a_sentence() {
        assert_eq!(answer_from(None), (false, OFFLINE.into()));
        assert_eq!(answer_from(Some((429, None))), (false, TOO_MANY.into()));
        assert_eq!(
            answer_from(Some((200, Some(json!({"ok": true}))))),
            (true, THANKS.into())
        );
        assert_eq!(answer_from(Some((200, None))), (false, GARBLED.into()));
        assert_eq!(
            answer_from(Some((
                400,
                Some(json!({"ok": false, "error": "name too long"}))
            ))),
            (false, "Could not send. name too long".into())
        );
    }

    #[test]
    fn the_address_is_the_suite_s_own_and_names_no_person() {
        assert_eq!(REQUEST_URL, "https://requests.methodreader.com/request");
        assert!(!REQUEST_URL.contains("workers.dev"));
    }
}
