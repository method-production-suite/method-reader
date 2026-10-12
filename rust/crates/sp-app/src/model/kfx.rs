//! The KFX checklist ("Best Kindle quality"): `desktop/ui/kfx.js`'s
//! decisions.

use serde_json::Value;

use super::json::{error_message, ok};
use super::send_routes::Device;

pub const HEADING: &str = "Best Kindle quality";
pub const INSTALLED: &str = "Installed";
pub const INSTALLING: &str = "Downloading and installing the KFX plugin. This takes a few seconds.";
pub const NO_REASON: &str = "The KFX plugin was not installed, and nothing said why.";

const IDS: [&str; 3] = ["calibre", "previewer", "plugin"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    Link { label: String, url: String },
    Install { label: String },
    After { why: String },
    Unavailable { why: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub id: &'static str,
    pub name: String,
    pub installed: bool,
    pub fix: Option<Fix>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checklist {
    pub ready: bool,
    pub possible: bool,
    pub summary: String,
    pub steps: Vec<Step>,
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// Some(None) for an explicit null, None for a broken fix.
fn fix_from(fix: &Value) -> Option<Option<Fix>> {
    if fix.is_null() {
        return Some(None);
    }
    let kind = fix.get("kind")?.as_str()?;
    Some(Some(match kind {
        "link" => Fix::Link {
            label: text(fix, "label")?,
            url: text(fix, "url")?,
        },
        "install" => Fix::Install {
            label: text(fix, "label")?,
        },
        "after" => Fix::After {
            why: text(fix, "why")?,
        },
        "unavailable" => Fix::Unavailable {
            why: text(fix, "why")?,
        },
        _ => return None,
    }))
}

fn step_from(step: &Value, id: &'static str) -> Option<Step> {
    if step.get("id")?.as_str()? != id {
        return None;
    }
    let name = text(step, "name")?;
    let installed = step.get("installed")?.as_bool()?;
    let fix = fix_from(step.get("fix")?)?;
    if fix.is_none() != installed {
        return None;
    }
    Some(Step {
        id,
        name,
        installed,
        fix,
    })
}

/// The checklist, whole, or None.
pub fn checklist_from(value: &Value) -> Option<Checklist> {
    let ready = value.get("ready")?.as_bool()?;
    let possible = value.get("possible")?.as_bool()?;
    let summary = text(value, "summary")?;
    let steps = value.get("steps")?.as_array()?;
    if steps.len() != IDS.len() {
        return None;
    }
    let steps = IDS
        .iter()
        .zip(steps)
        .map(|(id, s)| step_from(s, id))
        .collect::<Option<Vec<_>>>()?;
    Some(Checklist {
        ready,
        possible,
        summary,
        steps,
    })
}

pub fn setup_from(answer: &Value) -> Option<Checklist> {
    if ok(answer) {
        checklist_from(answer)
    } else {
        None
    }
}

/// Kindle advice is for Kindles: with nothing connected or a Kindle
/// connected; not yet known is not relevant yet.
pub fn kindle_relevant(devices: Option<&[Device]>) -> bool {
    devices.is_some_and(|d| d.is_empty() || d.iter().any(|x| x.kind == "kindle"))
}

pub fn show_setup(
    setup: Option<&Checklist>,
    devices: Option<&[Device]>,
    just_installed: bool,
    installing: bool,
) -> bool {
    let Some(setup) = setup.filter(|s| s.possible) else {
        return false;
    };
    if installing || just_installed {
        return true;
    }
    kindle_relevant(devices) && !setup.ready
}

/// What stands to the right of one step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Status(String),
    Link { label: String, url: String },
    Install { label: String, disabled: bool },
}

pub fn control_for(step: &Step, busy: bool) -> Control {
    match &step.fix {
        None => Control::Status(INSTALLED.into()),
        Some(Fix::Link { label, url }) => Control::Link {
            label: label.clone(),
            url: url.clone(),
        },
        Some(Fix::Install { label }) => Control::Install {
            label: label.clone(),
            disabled: busy,
        },
        Some(Fix::After { why } | Fix::Unavailable { why }) => Control::Status(why.clone()),
    }
}

pub fn installed_line(version: &str, removed: &[String], ready: bool) -> String {
    let mut parts = vec![format!("Installed the KFX plugin {version}.")];
    if ready {
        parts.push("Kindles now get KFX.".into());
    }
    let names: Vec<&str> = removed
        .iter()
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .collect();
    match names.as_slice() {
        [] => {}
        [one] => parts.push(format!("Removed an older copy: {one}.")),
        many => parts.push(format!("Removed older copies: {}.", many.join(", "))),
    }
    parts.join(" ")
}

pub fn failed_line(answer: &Value) -> String {
    error_message(answer).unwrap_or_else(|| NO_REASON.into())
}

pub fn link_failed_line(url: &str) -> String {
    format!("Could not open the page. It is at {url}")
}

/// Everything an install's answer changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AfterInstall {
    pub setup: Option<Checklist>,
    pub line: String,
    pub bad: bool,
    pub just_installed: bool,
}

pub fn after_install(answer: &Value, previous: Option<&Checklist>) -> AfterInstall {
    if ok(answer)
        && let Some(version) = text(answer, "version")
    {
        let setup = answer
            .get("setup")
            .and_then(checklist_from)
            .or_else(|| previous.cloned());
        let removed: Vec<String> = answer
            .get("removed")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let ready = setup.as_ref().is_some_and(|s| s.ready);
        return AfterInstall {
            line: installed_line(&version, &removed, ready),
            setup,
            bad: false,
            just_installed: true,
        };
    }
    AfterInstall {
        setup: previous.cloned(),
        line: failed_line(answer),
        bad: true,
        just_installed: false,
    }
}

#[cfg(test)]
pub mod fixtures {
    use serde_json::{Value, json};

    pub fn missing_plugin() -> Value {
        json!({"ok": true, "ready": false, "possible": true,
        "summary": "Kindles get AZW3 today. Add the KFX plugin for the best layout.",
        "steps": [
            {"id": "calibre", "name": "Calibre", "installed": true, "fix": null},
            {"id": "previewer", "name": "Kindle Previewer", "installed": false,
             "fix": {"kind": "link", "label": "Get it", "url": "https://kdp.amazon.com/en_US/help/topic/G202131170"}},
            {"id": "plugin", "name": "KFX plugin", "installed": false,
             "fix": {"kind": "install", "label": "Install"}}
        ]})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_checklist_is_taken_whole_or_draws_nothing() {
        let list = setup_from(&fixtures::missing_plugin()).unwrap();
        assert_eq!(list.steps.len(), 3);
        let mut out_of_order = fixtures::missing_plugin();
        out_of_order["steps"].as_array_mut().unwrap().swap(0, 1);
        assert!(setup_from(&out_of_order).is_none());
        let mut lying = fixtures::missing_plugin();
        lying["steps"][0]["installed"] = json!(false);
        assert!(
            setup_from(&lying).is_none(),
            "a fix exactly when not installed"
        );
        assert!(setup_from(&json!({"ok": false})).is_none());
    }

    #[test]
    fn it_shows_only_where_it_matters() {
        let list = setup_from(&fixtures::missing_plugin()).unwrap();
        let kobo = Device {
            id: "k".into(),
            kind: "kobo".into(),
            name: "Kobo".into(),
            volume: None,
        };
        assert!(show_setup(Some(&list), Some(&[]), false, false));
        assert!(!show_setup(
            Some(&list),
            Some(std::slice::from_ref(&kobo)),
            false,
            false
        ));
        assert!(
            show_setup(Some(&list), Some(&[kobo]), false, true),
            "an install runs"
        );
        assert!(
            !show_setup(Some(&list), None, false, false),
            "not known yet"
        );
        let ready = Checklist {
            ready: true,
            ..list.clone()
        };
        assert!(!show_setup(Some(&ready), Some(&[]), false, false));
        assert!(
            show_setup(Some(&ready), Some(&[]), true, false),
            "the success line needs room"
        );
        let impossible = Checklist {
            possible: false,
            ..list
        };
        assert!(!show_setup(Some(&impossible), Some(&[]), true, true));
    }

    #[test]
    fn each_step_gets_one_control() {
        let list = setup_from(&fixtures::missing_plugin()).unwrap();
        assert_eq!(
            control_for(&list.steps[0], false),
            Control::Status(INSTALLED.into())
        );
        assert!(matches!(
            control_for(&list.steps[1], true),
            Control::Link { .. }
        ));
        assert_eq!(
            control_for(&list.steps[2], true),
            Control::Install {
                label: "Install".into(),
                disabled: true
            }
        );
    }

    #[test]
    fn an_install_says_what_happened() {
        let ok_answer = json!({"ok": true, "version": "2.3", "removed": ["Old KFX"],
            "setup": {"ready": true, "possible": true, "summary": "Ready.",
                "steps": [
                    {"id": "calibre", "name": "Calibre", "installed": true, "fix": null},
                    {"id": "previewer", "name": "Kindle Previewer", "installed": true, "fix": null},
                    {"id": "plugin", "name": "KFX plugin", "installed": true, "fix": null}]}});
        let after = after_install(&ok_answer, None);
        assert_eq!(
            after.line,
            "Installed the KFX plugin 2.3. Kindles now get KFX. Removed an older copy: Old KFX."
        );
        assert!(after.just_installed && !after.bad);
        let mut broken = ok_answer.clone();
        broken["setup"] = json!({"ready": true});
        let previous = setup_from(&fixtures::missing_plugin());
        let kept = after_install(&broken, previous.as_ref());
        assert_eq!(kept.setup, previous, "a broken checklist keeps the old one");
        assert_eq!(
            kept.line,
            "Installed the KFX plugin 2.3. Removed an older copy: Old KFX."
        );
        let refused = after_install(&json!({"ok": false}), None);
        assert_eq!((refused.line.as_str(), refused.bad), (NO_REASON, true));
        assert_eq!(
            installed_line("1", &["a".into(), "b".into()], false),
            "Installed the KFX plugin 1. Removed older copies: a, b."
        );
        assert_eq!(
            link_failed_line("https://x"),
            "Could not open the page. It is at https://x"
        );
    }
}
