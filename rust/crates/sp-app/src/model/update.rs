//! Whether there is a newer Method Reader, and what to say about it:
//! was `desktop/ui/update.js`, `update-flow.js` and the Updates block's
//! rules from `notes-surface.js`. The transport (download, signature, swap,
//! restart) is wave 3's; everything decided here is injected-and-tested,
//! as it was in JavaScript.

use std::collections::HashMap;

use super::update_compare::{Pick, pick_update, should_check};

const OPT_IN: &str = "updateOptIn";
const ASKED: &str = "updateAsked";
const STAMP: &str = "updateLastChecked";
const FOUND: &str = "updateFound";

/// How long a restart waits for quiet before it hands the choice over.
pub const RESTART_WAIT_CAP_MS: f64 = 120_000.0;

pub const INTEL_MAC_RETIRED: &str = "New versions of Method Reader run only on Macs with Apple silicon, so this copy will not be offered updates. The version you have keeps working.";

/// The window's own string bucket (localStorage in the webview).
pub trait Storage {
    fn get_item(&self, key: &str) -> Option<String>;
    fn set_item(&mut self, key: &str, value: &str);
}

#[derive(Debug, Default, Clone)]
pub struct MemoryStorage(pub HashMap<String, String>);

impl Storage for MemoryStorage {
    fn get_item(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_item(&mut self, key: &str, value: &str) {
        self.0.insert(key.into(), value.into());
    }
}

/// Whether a check's rejection is the manifest having nothing for an
/// Intel Mac.
pub fn is_intel_mac_retired(message: &str) -> bool {
    message.contains("`platforms` object")
        && message.match_indices("darwin-x86_64").any(|(i, _)| {
            let before = message[..i].chars().next_back();
            let after = message[i + "darwin-x86_64".len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
}

/// Only on a Mac: the manifest lists darwin only.
pub fn updates_possible(platform: super::convert::Platform) -> bool {
    platform == super::convert::Platform::Mac
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub opted_in: bool,
    pub asked: bool,
    pub last_checked: Option<f64>,
}

pub fn read_state(storage: &dyn Storage) -> State {
    let raw = storage
        .get_item(STAMP)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n > 0.0);
    State {
        opted_in: storage.get_item(OPT_IN).as_deref() == Some("true"),
        asked: storage.get_item(ASKED).as_deref() == Some("true"),
        last_checked: raw,
    }
}

pub fn remember_answer(storage: &mut dyn Storage, opted_in: bool) {
    storage.set_item(ASKED, "true");
    storage.set_item(OPT_IN, if opted_in { "true" } else { "false" });
}

pub fn should_ask(usable: bool, storage: &dyn Storage) -> bool {
    usable && !read_state(storage).asked
}

pub fn remember_found(storage: &mut dyn Storage, version: &str) {
    storage.set_item(FOUND, version);
}

pub fn forget_found(storage: &mut dyn Storage) {
    storage.set_item(FOUND, "");
}

pub fn remembered_offer(storage: &dyn Storage, current: &str) -> Option<String> {
    let found = storage.get_item(FOUND).filter(|f| !f.is_empty())?;
    match pick_update(Some((&found, current))) {
        Pick::Offer(_) => Some(found),
        Pick::No(_) => None,
    }
}

pub fn download_percent(received: f64, total: Option<f64>) -> Option<u8> {
    let total = total.filter(|t| t.is_finite() && *t > 0.0)?;
    if !received.is_finite() {
        return None;
    }
    Some(((received / total) * 100.0).floor().clamp(0.0, 100.0) as u8)
}

/// Each moment of an update.
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Offer {
        version: String,
        body: String,
        retrying: bool,
    },
    Downloading {
        version: String,
        received: f64,
        total: Option<f64>,
        body: String,
    },
    Installing {
        version: String,
    },
    Waiting {
        version: String,
    },
    Stalled {
        version: String,
    },
    Restarting {
        version: String,
    },
    Failed {
        version: Option<String>,
        message: String,
    },
    Installed {
        version: String,
    },
}

impl Phase {
    pub fn version(&self) -> Option<&str> {
        match self {
            Phase::Offer { version, .. }
            | Phase::Downloading { version, .. }
            | Phase::Installing { version }
            | Phase::Waiting { version }
            | Phase::Stalled { version }
            | Phase::Restarting { version }
            | Phase::Installed { version } => Some(version),
            Phase::Failed { version, .. } => version.as_deref(),
        }
    }
}

pub fn installed_line(version: &str) -> String {
    format!("Update installed. Quit and reopen Method Reader to use {version}.")
}

/// The words beside the stamp, or None.
pub fn update_label(phase: Option<&Phase>) -> Option<String> {
    Some(match phase? {
        Phase::Offer { version, .. } => format!("Update to {version}"),
        Phase::Downloading {
            version,
            received,
            total,
            ..
        } => match download_percent(*received, *total) {
            Some(p) => format!("Downloading {version}… {p}%"),
            None => format!("Downloading {version}…"),
        },
        Phase::Installing { .. } => "Installing…".into(),
        Phase::Waiting { .. } => "Restarting after this finishes…".into(),
        Phase::Stalled { .. } => "Restart now".into(),
        Phase::Restarting { .. } => "Restarting…".into(),
        Phase::Failed { .. } => "Update failed. Try again".into(),
        Phase::Installed { version } => installed_line(version),
    })
}

/// Whether a click on the label would act.
pub fn label_actionable(phase: Option<&Phase>) -> bool {
    match phase {
        Some(Phase::Offer { retrying, .. }) => !retrying,
        Some(Phase::Failed { .. } | Phase::Stalled { .. }) => true,
        _ => false,
    }
}

/// What a check found, as the updater reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CheckOutcome {
    Skipped,
    Offer { version: String, body: String },
    Current { reason: String },
    Error { message: String },
}

/// Run a check, or decline to. The stamp is written BEFORE the request.
pub fn run_check(
    manual: bool,
    storage: &mut dyn Storage,
    now: f64,
    check: impl FnOnce() -> Result<Option<Found>, String>,
) -> CheckOutcome {
    if !begin_check(manual, storage, now) {
        return CheckOutcome::Skipped;
    }
    finish_check(storage, check())
}

/// [`run_check`]'s first half, on the window's thread: whether to ask at
/// all, and if so the stamp, written before the request goes out.
pub fn begin_check(manual: bool, storage: &mut dyn Storage, now: f64) -> bool {
    let state = read_state(storage);
    if !manual && !should_check(state.opted_in, state.last_checked, now) {
        return false;
    }
    storage.set_item(STAMP, &now.to_string());
    true
}

/// [`run_check`]'s second half: what the answer means, remembered. The
/// request itself runs between the two, off the window's thread.
pub fn finish_check(
    storage: &mut dyn Storage,
    answer: Result<Option<Found>, String>,
) -> CheckOutcome {
    let found = match answer {
        Ok(found) => found,
        Err(message) => {
            if is_intel_mac_retired(&message) {
                forget_found(storage);
                return CheckOutcome::Error {
                    message: INTEL_MAC_RETIRED.into(),
                };
            }
            return CheckOutcome::Error { message };
        }
    };
    let pick = pick_update(
        found
            .as_ref()
            .map(|f| (f.version.as_str(), f.current_version.as_str())),
    );
    match (pick, found) {
        (Pick::Offer(version), Some(f)) => {
            remember_found(storage, &version);
            CheckOutcome::Offer {
                version,
                body: f.body.unwrap_or_default(),
            }
        }
        (Pick::No(reason), _) | (Pick::Offer(reason), None) => {
            forget_found(storage);
            CheckOutcome::Current { reason }
        }
    }
}

/// The Updates block in the release notes, for one moment.
#[derive(Clone, Debug, PartialEq)]
pub struct NotesView {
    /// None: leave the line as it is.
    pub say: Option<String>,
    pub body: Option<String>,
    pub install: Option<(String, bool)>,
    pub check_disabled: bool,
}

/// What the block has drawn so far this session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawn {
    pub drew_offer: bool,
    pub version: Option<String>,
    pub body: String,
}

pub fn notes_view(phase: Option<&Phase>, drawn: &Drawn, running: &str) -> NotesView {
    let Some(phase) = phase else {
        return NotesView {
            say: drawn
                .drew_offer
                .then(|| format!("Method Reader {running} is the newest there is.")),
            body: None,
            install: None,
            check_disabled: false,
        };
    };
    if let Phase::Offer { version, body, .. } = phase {
        return NotesView {
            say: Some(format!("Method Reader {version} is available.")),
            body: (!body.is_empty()).then(|| body.clone()),
            install: Some((format!("Install {version}"), !label_actionable(Some(phase)))),
            check_disabled: false,
        };
    }
    let version = phase
        .version()
        .map(str::to_string)
        .or_else(|| drawn.version.clone())
        .unwrap_or_default();
    let body = match phase {
        Phase::Downloading { body, .. } => body.clone(),
        _ => drawn.body.clone(),
    };
    let check_disabled = matches!(
        phase,
        Phase::Downloading { .. }
            | Phase::Installing { .. }
            | Phase::Waiting { .. }
            | Phase::Stalled { .. }
            | Phase::Restarting { .. }
            | Phase::Installed { .. }
    );
    let body = (!body.is_empty()).then_some(body);
    if let Phase::Stalled { .. } = phase {
        return NotesView {
            say: Some(format!(
                "Method Reader {version} is installed, but something is still running. Restart when you are ready."
            )),
            body,
            install: Some((update_label(Some(phase)).unwrap_or_default(), false)),
            check_disabled,
        };
    }
    NotesView {
        say: Some(match phase {
            Phase::Failed { message, .. } => message.clone(),
            _ => update_label(Some(phase)).unwrap_or_default(),
        }),
        body,
        install: Some((format!("Install {version}"), !label_actionable(Some(phase)))),
        check_disabled,
    }
}

pub const NOT_HERE: &str = "Method Reader does not update itself here. New releases are at github.com/method-production-suite/method-reader/releases.";
pub const AUTO_LABEL: &str =
    "Look for a new version once a day. Off by default; nothing is sent but the request itself.";
pub const AUTO_ON: &str = "Method Reader will look once a day, and only for this file.";
pub const AUTO_OFF: &str = "Method Reader will not look on its own.";
pub const CHECK_BUTTON: &str = "Check for updates";
pub const LOOKING: &str = "Looking…";

#[cfg(test)]
mod tests {
    use super::*;

    fn found(v: &str) -> Found {
        Found {
            version: v.into(),
            current_version: "0.11.1".into(),
            body: Some("notes".into()),
        }
    }

    #[test]
    fn the_question_is_asked_once_and_said_no_differs_from_never_asked() {
        let mut s = MemoryStorage::default();
        assert!(should_ask(true, &s));
        assert!(!should_ask(false, &s));
        remember_answer(&mut s, false);
        assert!(!should_ask(true, &s));
        assert_eq!(
            read_state(&s),
            State {
                opted_in: false,
                asked: true,
                last_checked: None
            }
        );
    }

    #[test]
    fn a_junk_stamp_reads_as_never_checked() {
        let mut s = MemoryStorage::default();
        s.set_item(STAMP, "NaN");
        assert_eq!(read_state(&s).last_checked, None);
        s.set_item(STAMP, "-4");
        assert_eq!(read_state(&s).last_checked, None);
    }

    #[test]
    fn the_stamp_is_written_before_the_request_and_a_failure_is_a_message() {
        let mut s = MemoryStorage::default();
        remember_answer(&mut s, true);
        let out = run_check(false, &mut s, 1000.0, || Err("offline".into()));
        assert_eq!(
            out,
            CheckOutcome::Error {
                message: "offline".into()
            }
        );
        assert_eq!(read_state(&s).last_checked, Some(1000.0));
        assert_eq!(
            run_check(false, &mut s, 2000.0, || panic!("throttled")),
            CheckOutcome::Skipped
        );
        let manual = run_check(true, &mut s, 2000.0, || Ok(Some(found("0.12.0"))));
        assert_eq!(
            manual,
            CheckOutcome::Offer {
                version: "0.12.0".into(),
                body: "notes".into()
            }
        );
        assert_eq!(remembered_offer(&s, "0.11.1").as_deref(), Some("0.12.0"));
        assert_eq!(remembered_offer(&s, "0.12.0"), None);
    }

    #[test]
    fn not_opted_in_never_asks_on_its_own() {
        let mut s = MemoryStorage::default();
        assert_eq!(
            run_check(false, &mut s, 1.0, || panic!("no")),
            CheckOutcome::Skipped
        );
    }

    #[test]
    fn nothing_newer_forgets_what_was_remembered_and_a_failure_does_not() {
        let mut s = MemoryStorage::default();
        remember_found(&mut s, "0.12.0");
        let _ = run_check(true, &mut s, 1.0, || Err("down".into()));
        assert_eq!(remembered_offer(&s, "0.11.1").as_deref(), Some("0.12.0"));
        let current = run_check(true, &mut s, 2.0, || Ok(None));
        assert!(matches!(current, CheckOutcome::Current { .. }));
        assert_eq!(remembered_offer(&s, "0.11.1"), None);
    }

    #[test]
    fn an_intel_mac_is_told_in_plain_words() {
        let mut s = MemoryStorage::default();
        remember_found(&mut s, "0.12.0");
        let msg = "None of the fallback platforms `[\"darwin-x86_64-app\", \"darwin-x86_64\"]` were found in the response `platforms` object";
        assert!(is_intel_mac_retired(msg));
        assert_eq!(
            run_check(true, &mut s, 1.0, || Err(msg.into())),
            CheckOutcome::Error {
                message: INTEL_MAC_RETIRED.into()
            }
        );
        assert_eq!(remembered_offer(&s, "0.11.1"), None);
        assert!(!is_intel_mac_retired(
            "darwin-aarch64 missing from `platforms` object"
        ));
    }

    #[test]
    fn the_label_says_each_moment_and_is_clickable_only_when_a_click_acts() {
        let v = || "0.12.0".to_string();
        let offer = Phase::Offer {
            version: v(),
            body: String::new(),
            retrying: false,
        };
        assert_eq!(
            update_label(Some(&offer)).as_deref(),
            Some("Update to 0.12.0")
        );
        assert!(label_actionable(Some(&offer)));
        let retry = Phase::Offer {
            version: v(),
            body: String::new(),
            retrying: true,
        };
        assert!(!label_actionable(Some(&retry)));
        let dl = Phase::Downloading {
            version: v(),
            received: 40.0,
            total: Some(100.0),
            body: String::new(),
        };
        assert_eq!(
            update_label(Some(&dl)).as_deref(),
            Some("Downloading 0.12.0… 40%")
        );
        assert!(!label_actionable(Some(&dl)));
        let unknown = Phase::Downloading {
            version: v(),
            received: 4.0,
            total: None,
            body: String::new(),
        };
        assert_eq!(
            update_label(Some(&unknown)).as_deref(),
            Some("Downloading 0.12.0…")
        );
        assert_eq!(download_percent(140.0, Some(100.0)), Some(100));
        assert_eq!(
            update_label(Some(&Phase::Waiting { version: v() })).as_deref(),
            Some("Restarting after this finishes…")
        );
        assert_eq!(
            update_label(Some(&Phase::Stalled { version: v() })).as_deref(),
            Some("Restart now")
        );
        assert!(label_actionable(Some(&Phase::Stalled { version: v() })));
        assert_eq!(
            update_label(Some(&Phase::Installed { version: v() })).as_deref(),
            Some("Update installed. Quit and reopen Method Reader to use 0.12.0.")
        );
        assert_eq!(update_label(None), None);
    }

    #[test]
    fn the_notes_block_says_what_the_label_says() {
        let idle = notes_view(None, &Drawn::default(), "0.11.1");
        assert_eq!(idle.say, None);
        let after = notes_view(
            None,
            &Drawn {
                drew_offer: true,
                ..Default::default()
            },
            "0.11.1",
        );
        assert_eq!(
            after.say.as_deref(),
            Some("Method Reader 0.11.1 is the newest there is.")
        );
        let offer = Phase::Offer {
            version: "0.12.0".into(),
            body: "b".into(),
            retrying: false,
        };
        let shown = notes_view(Some(&offer), &Drawn::default(), "0.11.1");
        assert_eq!(shown.install, Some(("Install 0.12.0".into(), false)));
        assert_eq!(shown.body.as_deref(), Some("b"));
        let installing = notes_view(
            Some(&Phase::Installing {
                version: "0.12.0".into(),
            }),
            &Drawn {
                drew_offer: true,
                version: Some("0.12.0".into()),
                body: "b".into(),
            },
            "0.11.1",
        );
        assert!(installing.check_disabled);
        assert_eq!(
            installing.body.as_deref(),
            Some("b"),
            "the notes stay while it installs"
        );
        let failed = notes_view(
            Some(&Phase::Failed {
                version: None,
                message: "no disk".into(),
            }),
            &Drawn::default(),
            "0.11.1",
        );
        assert_eq!(failed.say.as_deref(), Some("no disk"));
        assert!(!failed.check_disabled);
        let stalled = notes_view(
            Some(&Phase::Stalled {
                version: "0.12.0".into(),
            }),
            &Drawn::default(),
            "0.11.1",
        );
        assert_eq!(stalled.install, Some(("Restart now".into(), false)));
    }
}
