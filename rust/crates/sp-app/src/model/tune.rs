//! What the Settings page decides (was `desktop/ui/tune.js`, above its line).

use serde_json::{Map, Value};

use super::json::{error_message, ok};

pub type Settings = Map<String, Value>;

/// Why a knob can be here and still not change the page behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Live,
    Book,
    Reconvert,
}

/// What kind of control a knob is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Range {
        min: f64,
        max: f64,
        step: f64,
        unit: &'static str,
    },
    Toggle,
    Choice(&'static [(&'static str, &'static str)]),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Knob {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub help: Option<&'static str>,
    /// Another knob's key and the value it must hold for this one to matter.
    pub needs: Option<(&'static str, &'static str)>,
    pub beta: bool,
    pub effect: Effect,
}

pub struct Group {
    pub id: &'static str,
    pub title: &'static str,
    pub note: Option<&'static str>,
    pub knobs: Vec<Knob>,
}

const fn knob(key: &'static str, label: &'static str, kind: Kind) -> Knob {
    Knob {
        key,
        label,
        kind,
        help: None,
        needs: None,
        beta: false,
        effect: Effect::Live,
    }
}

impl Knob {
    const fn help(mut self, help: &'static str) -> Self {
        self.help = Some(help);
        self
    }
    const fn needs(mut self, key: &'static str, value: &'static str) -> Self {
        self.needs = Some((key, value));
        self
    }
    const fn beta(mut self) -> Self {
        self.beta = true;
        self
    }
    const fn effect(mut self, effect: Effect) -> Self {
        self.effect = effect;
        self
    }
}

const CUE_CHOICES: &[(&str, &str)] = &[("centered", "Centered"), ("indented", "Indented")];
const FONT_CHOICES: &[(&str, &str)] =
    &[("courier", "Courier"), ("serif", "Serif"), ("sans", "Sans")];
const CONTD_CHOICES: &[(&str, &str)] = &[
    ("auto", "Automatic"),
    ("strip", "Remove all"),
    ("keep", "Keep as written"),
];
const DUAL_CHOICES: &[(&str, &str)] = &[
    ("sideBySide", "Side by side"),
    ("sequential", "One after the other"),
];

/// The twenty, grouped by what a reader is trying to change.
pub fn groups() -> Vec<Group> {
    vec![
        Group {
            id: "page",
            title: "The page",
            note: None,
            knobs: vec![
                knob(
                    "elementSpacingEm",
                    "Space between elements",
                    Kind::Range { min: 0.4, max: 2.0, step: 0.1, unit: " em" },
                ),
                knob("scenePageBreaks", "Start each scene on a new page", Kind::Toggle).help(
                    "Makes every scene easy to find, and makes the book considerably longer.",
                ),
                knob("keepSceneHeadingWithScene", "Never end a page on a scene heading", Kind::Toggle).help(
                    "A heading alone at the foot of a page announces a scene and then makes you turn over to find it.",
                ),
                knob("keepSpeechesWhole", "Keep each speech on one page", Kind::Toggle).help(
                    "Stops a page turn landing in the middle of what someone is saying. The cost is a gap at the bottom of some pages. A speech longer than a whole page still has to break somewhere.",
                ),
                knob("printSplitMinimums", "Avoid stranded lines", Kind::Toggle).help(
                    "Stops a single line of a speech or a paragraph being left behind at the top or bottom of a page. Turning it off fits a little more onto each page. Not every e-reader obeys this one.",
                ),
            ],
        },
        Group {
            id: "dialogue",
            title: "Dialogue",
            note: None,
            knobs: vec![
                knob(
                    "dialogueSideMarginPct",
                    "Dialogue margins",
                    Kind::Range { min: 0.0, max: 30.0, step: 1.0, unit: "%" },
                ),
                knob("cueAlignment", "Character names", Kind::Choice(CUE_CHOICES)).help(
                    "Centred looks right at any screen size. Indented copies where they sit on a printed page, which only lines up at one width.",
                ),
                knob(
                    "cueIndentPct",
                    "Character name indent",
                    Kind::Range { min: 0.0, max: 60.0, step: 1.0, unit: "%" },
                )
                .needs("cueAlignment", "indented"),
                knob(
                    "parentheticalIndentPct",
                    "Parenthetical indent",
                    Kind::Range { min: 0.0, max: 40.0, step: 1.0, unit: "%" },
                )
                .needs("cueAlignment", "indented"),
            ],
        },
        Group {
            id: "text",
            title: "The text",
            note: None,
            knobs: vec![
                knob("fontFamily", "Typeface", Kind::Choice(FONT_CHOICES)),
                knob("justifyText", "Straighten the right edge", Kind::Toggle).help(
                    "Screenplays normally leave the right edge uneven. Straightening it opens up wide gaps between words in a column this narrow.",
                ),
                knob("preserveFontShifts", "Keep the script's own type changes", Kind::Toggle).help(
                    "Some scripts set titles, inserts and on-screen text in a different typeface or size. This keeps them the way the script drew them; turning it off puts every line in one typeface.",
                ),
            ],
        },
        Group {
            id: "front",
            title: "What the book carries",
            note: None,
            knobs: vec![
                knob("includeTitlePage", "Title page", Kind::Toggle)
                    .effect(Effect::Book)
                    .help("It appears in the finished book, not in the preview here, because the preview is the script itself."),
                knob("showSceneNumbers", "Scene numbers", Kind::Toggle).help(
                    "Only shows numbers the script already carried. Method Reader never invents them: a numbered draft is a decision someone made, not a formatting choice.",
                ),
                knob("includeImages", "Pictures from the script", Kind::Toggle).beta().help(
                    "Shows the drawings and photos the PDF carried, in the book and in the preview, where they sat in the script (a title card can become the cover instead, below). The Kindle file Method Reader makes without Calibre leaves them out for now. A script with no pictures makes the same book either way.",
                ),
                knob("promoteCoverImage", "Title card as the cover", Kind::Toggle).beta().help(
                    "When a script opens with a full-page picture before its first scene, the book uses it as its cover instead of its first page, so the preview leaves it out. Needs Pictures from the script on. A script without one makes the same book either way.",
                ),
            ],
        },
        Group {
            id: "pdf",
            title: "Read from the PDF",
            note: Some("These four are decided while the PDF is being read. Everything else re-renders from the script Method Reader already read, so changing one here saves it for the next time you convert this PDF. It will not change what you see now."),
            knobs: vec![
                knob("rejoinSplitDialogue", "Rejoin speeches split across pages", Kind::Toggle)
                    .effect(Effect::Reconvert)
                    .help("A printed script breaks a long speech across two pages and marks it (MORE) and (CONT'D). This stitches the halves back into one speech. Turning it off keeps the break exactly where the PDF had it."),
                knob("contdMode", "(CONT'D) after a cue", Kind::Choice(CONTD_CHOICES))
                    .effect(Effect::Reconvert),
                knob("showPageMarkers", "The PDF's page numbers", Kind::Toggle)
                    .effect(Effect::Reconvert)
                    .help("Marks where each page of the printed script ended, so a note about page 42 can still be found."),
                knob("dualDialogue", "People talking at once", Kind::Choice(DUAL_CHOICES))
                    .effect(Effect::Reconvert)
                    .help("Side by side needs two columns, which is a sliver on a phone. One after the other reads anywhere."),
            ],
        },
    ]
}

/// Every knob, in the order it is drawn.
pub fn knobs() -> Vec<Knob> {
    groups().into_iter().flat_map(|g| g.knobs).collect()
}

pub fn option_keys() -> Vec<&'static str> {
    knobs().into_iter().map(|k| k.key).collect()
}

pub fn knob_for(key: &str) -> Option<Knob> {
    knobs().into_iter().find(|k| k.key == key)
}

/// Why this knob does nothing right now, or None.
pub fn idle_reason(knob: &Knob, settings: Option<&Settings>) -> Option<String> {
    let (key, want) = knob.needs?;
    let settings = settings?;
    if settings.get(key).and_then(Value::as_str) == Some(want) {
        return None;
    }
    let other = knob_for(key);
    let label = other
        .as_ref()
        .and_then(|o| match &o.kind {
            Kind::Choice(choices) => choices.iter().find(|(v, _)| *v == want).map(|(_, l)| *l),
            _ => None,
        })
        .unwrap_or(want);
    let name = other.map(|o| o.label).unwrap_or(key);
    Some(format!(
        "Only when {} are {}.",
        name.to_lowercase(),
        label.to_lowercase()
    ))
}

/// Everything beside a control right now: why it is idle, then what it is
/// for.
pub fn notes_for(knob: &Knob, settings: Option<&Settings>) -> Vec<String> {
    [idle_reason(knob, settings), knob.help.map(str::to_string)]
        .into_iter()
        .flatten()
        .filter(|n| !n.trim().is_empty())
        .collect()
}

pub fn can_explain(knob: &Knob) -> bool {
    knob.needs.is_some() || knob.help.is_some()
}

pub const NOT_SET: &str = "not set";

/// What a range's read-out says.
pub fn display_value(knob: &Knob, value: Option<&Value>) -> String {
    let Kind::Range { step, unit, .. } = knob.kind else {
        return NOT_SET.into();
    };
    let Some(number) = value.and_then(Value::as_f64) else {
        return NOT_SET.into();
    };
    let decimals = if step < 1.0 { 1 } else { 0 };
    format!("{number:.decimals$}{unit}")
}

/// One moved control as a value the engine will accept, or None.
pub fn coerce_value(knob: &Knob, raw: &Value) -> Option<Value> {
    match &knob.kind {
        Kind::Toggle => raw.as_bool().map(Value::Bool),
        Kind::Range { min, max, .. } => {
            let number = match raw {
                Value::Number(n) => n.as_f64()?,
                Value::String(s) => s.trim().parse::<f64>().ok()?,
                _ => return None,
            };
            if !number.is_finite() {
                return None;
            }
            serde_json::Number::from_f64(number.clamp(*min, *max)).map(Value::Number)
        }
        Kind::Choice(choices) => {
            let value = raw.as_str()?;
            choices
                .iter()
                .any(|(c, _)| *c == value)
                .then(|| Value::String(value.into()))
        }
    }
}

/// What is owed: every change accumulates.
pub fn merge_pending(pending: &Settings, key: &str, value: Value) -> Settings {
    let mut next = pending.clone();
    next.insert(key.into(), value);
    next
}

/// What is owed after a save that did not happen: newer changes win.
pub fn restore_pending(owed: &Settings, pending: &Settings) -> Settings {
    let mut next = owed.clone();
    for (k, v) in pending {
        next.insert(k.clone(), v.clone());
    }
    next
}

/// The engine's settings answer, checked key by key, or None. Extra keys
/// survive.
pub fn settings_from(answer: &Value) -> Option<Settings> {
    let settings = answer.get("settings")?.as_object()?;
    for knob in knobs() {
        let value = settings.get(knob.key);
        let good = match &knob.kind {
            Kind::Toggle => value.is_some_and(Value::is_boolean),
            Kind::Range { .. } => value.is_some_and(Value::is_number),
            Kind::Choice(choices) => value
                .and_then(Value::as_str)
                .is_some_and(|v| choices.iter().any(|(c, _)| *c == v)),
        };
        if !good {
            return None;
        }
    }
    Some(settings.clone())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub id: String,
    pub display_name: String,
    pub settings: Settings,
}

/// The named starting points, from the engine's own list.
pub fn presets_from(answer: &Value) -> Vec<Preset> {
    let Some(list) = answer.get("presets").and_then(Value::as_array) else {
        return vec![];
    };
    list.iter()
        .filter_map(|p| {
            let display_name = p.get("displayName").and_then(Value::as_str).unwrap_or("");
            let settings = settings_from(p)?;
            (!display_name.is_empty()).then(|| Preset {
                id: p.get("id").and_then(Value::as_str).unwrap_or("").into(),
                display_name: display_name.into(),
                settings,
            })
        })
        .collect()
}

/// Which preset the script is on, by the engine's own comparison.
pub fn current_preset(answer: &Value) -> Option<String> {
    answer
        .get("preset")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn same_settings(a: Option<&Settings>, b: Option<&Settings>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => option_keys().iter().all(|k| a.get(*k) == b.get(*k)),
        (None, None) => true,
        _ => false,
    }
}

pub const STATUS_PENDING: &str = "Changed. Saving in a moment…";
pub const STATUS_SAVING: &str = "Saving, and rebuilding the book to match…";
pub const STATUS_SAVED: &str = "Saved. The book on disk matches what you see.";

/// What a save waiting its turn says, by what holds the book, longest
/// wait first.
pub const WAITING: &[(&str, &str)] = &[
    ("kindle", "Waiting for the Kindle file to finish building…"),
    ("convert", "Waiting for the conversion to finish…"),
    ("send", "Waiting for the send to finish…"),
    ("copy", "Waiting for the copy to finish…"),
];

pub const NO_MESSAGE: &str = "The engine refused the change without saying why.";

/// A status line: its words and whether it is an alarm.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Status {
    pub line: String,
    pub bad: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase<'a> {
    Idle,
    Pending,
    Saving,
    Saved,
    Waiting(&'a [&'a str]),
    Failed(Option<&'a str>),
}

pub fn status_for(phase: Phase<'_>) -> Status {
    let good = |line: &str| Status {
        line: line.into(),
        bad: false,
    };
    match phase {
        Phase::Idle => good(""),
        Phase::Pending => good(STATUS_PENDING),
        Phase::Saving => good(STATUS_SAVING),
        Phase::Saved => good(STATUS_SAVED),
        Phase::Waiting(labels) => good(
            WAITING
                .iter()
                .find(|(l, _)| labels.contains(l))
                .map(|(_, w)| *w)
                .unwrap_or(STATUS_PENDING),
        ),
        Phase::Failed(message) => {
            let said = message.map(str::trim).unwrap_or("");
            Status {
                line: if said.is_empty() {
                    NO_MESSAGE.into()
                } else {
                    said.into()
                },
                bad: true,
            }
        }
    }
}

/// A rebuild's warnings, each in parentheses.
pub fn warning_lines(answer: &Value) -> Vec<String> {
    answer
        .get("warnings")
        .and_then(Value::as_array)
        .map(|w| {
            w.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| format!("({s})"))
                .collect()
        })
        .unwrap_or_default()
}

pub const FAULT_SLUG: &str = "Settings out of reach";
pub const FAULT_LINE: &str = "Method Reader could not read this script’s settings. The book itself is fine and can still be sent; converting the PDF again is the way to get the knobs back.";
pub const FAULT_WAY: &str = "Convert it again";

pub const LEDE: &str = "These settings belong to this script alone. They are kept with it in your library, never beside the PDF you dropped, and the book on disk is rebuilt to match, so what you send is what you see.";
pub const HEADING: &str = "This script’s settings";
pub const PRESET_LABEL: &str = "Load a preset";
pub const PRESET_NOTE: &str = "Overwrites every setting below.";
pub const READING: &str = "Reading this script’s settings…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneMode {
    Ready,
    Reading,
    Fault,
}

/// The fault screen only for a draw that is itself reporting a failure.
pub fn empty_pane_mode(has_settings: bool, status: Option<&Status>) -> PaneMode {
    if has_settings {
        PaneMode::Ready
    } else if status.is_some_and(|s| s.bad) {
        PaneMode::Fault
    } else {
        PaneMode::Reading
    }
}

pub fn app_defaults_from(answer: &Value) -> Option<Settings> {
    settings_from(&serde_json::json!({ "settings": answer.get("appDefaults") }))
}

pub fn shipped_defaults_from(answer: &Value) -> Option<Settings> {
    settings_from(&serde_json::json!({ "settings": answer.get("defaults") }))
}

/// What a defaults write shows once app-settings --set has answered.
pub fn app_settings_after(answer: &Value) -> Result<(Settings, Settings), String> {
    let app = settings_from(&serde_json::json!({ "settings": answer.get("formatDefaults") }));
    let shipped = settings_from(&serde_json::json!({ "settings": answer.get("shippedDefaults") }));
    match (app, shipped) {
        (Some(a), Some(s)) => Ok((a, s)),
        _ => Err(error_message(answer).unwrap_or_else(|| NO_MESSAGE.into())),
    }
}

pub fn defaults_caption(app: Option<&Settings>, shipped: Option<&Settings>) -> &'static str {
    if same_settings(app, shipped) {
        "New scripts start from Method Reader’s own defaults."
    } else {
        "New scripts start from your own defaults."
    }
}

pub fn can_reset_defaults(app: Option<&Settings>, shipped: Option<&Settings>) -> bool {
    app.is_some() && shipped.is_some() && !same_settings(app, shipped)
}

pub fn defaults_write_args(next: Option<&Settings>) -> String {
    serde_json::json!({ "formatDefaults": next }).to_string()
}

pub const USE_DEFAULTS_LABEL: &str = "Use these for new scripts";
pub const RESET_DEFAULTS_LABEL: &str = "Reset new scripts to Method Reader’s defaults";
pub const USE_DEFAULTS_NOTE: &str = "New scripts will start from these settings.";
pub const RESET_DEFAULTS_NOTE: &str = "New scripts will start from Method Reader’s own defaults.";

/// What a defaults write becomes: applied, refused, or stale.
#[derive(Clone, Debug, PartialEq)]
pub enum DefaultsOutcome {
    Stale,
    Applied {
        message: String,
        app: Settings,
        shipped: Settings,
    },
    Refused(String),
}

pub fn defaults_write_outcome(
    mine: u64,
    current: u64,
    answer: &Value,
    confirmed: &str,
) -> DefaultsOutcome {
    if mine != current {
        return DefaultsOutcome::Stale;
    }
    match app_settings_after(answer) {
        Ok((app, shipped)) => DefaultsOutcome::Applied {
            message: confirmed.into(),
            app,
            shipped,
        },
        Err(message) => DefaultsOutcome::Refused(message),
    }
}

pub struct KeepOption {
    pub keep: bool,
    pub id: &'static str,
    pub label: &'static str,
    pub line: &'static str,
}

pub const KEEP_LEGEND: &str = "When a PDF is converted";
pub const KEEP_OPTIONS: [KeepOption; 2] = [
    KeepOption {
        keep: true,
        id: "keep-script-settings-keep",
        label: "Keep its settings",
        line: "The script keeps the settings it was made with. Changing the defaults later won’t change it.",
    },
    KeepOption {
        keep: false,
        id: "keep-script-settings-follow",
        label: "Follow the defaults",
        line: "Scripts you haven’t tuned change when the defaults do. Scripts that already have their own settings keep them.",
    },
];
pub const KEEP_SAVED_NOTE: &str = "Saved. It applies to PDFs you convert from now on.";

pub fn keep_choice_from(answer: &Value) -> Option<bool> {
    answer.get("keepScriptSettings").and_then(Value::as_bool)
}

pub fn keep_write_args(keep: bool) -> String {
    serde_json::json!({ "keepScriptSettings": keep }).to_string()
}

/// What one keep-choice write becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeepOutcome {
    pub stale: bool,
    pub stored: Option<bool>,
    pub message: String,
}

pub fn keep_write_outcome(
    mine: u64,
    current: u64,
    seq: u64,
    latest: u64,
    answer: &Value,
) -> KeepOutcome {
    if mine != current {
        return KeepOutcome {
            stale: true,
            stored: None,
            message: String::new(),
        };
    }
    let stored = if ok(answer) {
        keep_choice_from(answer)
    } else {
        None
    };
    if seq != latest {
        return KeepOutcome {
            stale: true,
            stored,
            message: String::new(),
        };
    }
    match stored {
        Some(_) => KeepOutcome {
            stale: false,
            stored,
            message: KEEP_SAVED_NOTE.into(),
        },
        None => KeepOutcome {
            stale: false,
            stored: None,
            message: error_message(answer).unwrap_or_else(|| NO_MESSAGE.into()),
        },
    }
}

pub struct SwitchOption {
    pub key: &'static str,
    pub id: &'static str,
    pub label: &'static str,
    pub line: &'static str,
}

pub const SWITCHES_LEGEND: &str = "While a script converts";
pub const SWITCH_OPTIONS: [SwitchOption; 2] = [
    SwitchOption {
        key: "animatePage",
        id: "page-switch-animate",
        label: "Animate the page",
        line: "The first page types itself out while the book is made.",
    },
    SwitchOption {
        key: "typewriterSounds",
        id: "page-switch-sounds",
        label: "Typewriter sounds",
        line: "Key clicks while the page types. Only when the page is animated.",
    },
];
pub const SWITCHES_REDUCED: &str =
    "This computer is set to reduce motion, so the page does not animate.";

/// What one switch write becomes: the two switches the engine now holds,
/// or its sentence.
pub fn switch_write_outcome(answer: &Value) -> Result<(bool, bool), String> {
    if ok(answer)
        && let Some(stored) = super::page_show::page_switches_from(answer)
    {
        return Ok(stored);
    }
    Err(error_message(answer).unwrap_or_else(|| NO_MESSAGE.into()))
}

/// Which groups arrive open: only the first.
pub fn group_starts_open(index: usize) -> bool {
    index == 0
}

/// Method Reader's own defaults: the root format-defaults.json, the one file
/// src/options.ts (and sp-model after it) is pinned to.
pub mod fixtures {
    use serde_json::Value;

    pub const DEFAULTS_JSON: &str = include_str!("../../../../../format-defaults.json");

    pub fn defaults() -> Value {
        serde_json::from_str(DEFAULTS_JSON).unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings(v: Value) -> Settings {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn there_are_twenty_knobs_in_five_groups() {
        assert_eq!(groups().len(), 5);
        assert_eq!(knobs().len(), 20);
        let keys = option_keys();
        let unique: std::collections::BTreeSet<_> = keys.iter().collect();
        assert_eq!(unique.len(), 20);
    }

    #[test]
    fn the_four_read_from_the_pdf_say_so() {
        let reconvert: Vec<_> = knobs()
            .into_iter()
            .filter(|k| k.effect == Effect::Reconvert)
            .map(|k| k.key)
            .collect();
        assert_eq!(
            reconvert,
            vec![
                "rejoinSplitDialogue",
                "contdMode",
                "showPageMarkers",
                "dualDialogue"
            ]
        );
        assert_eq!(knob_for("includeTitlePage").unwrap().effect, Effect::Book);
        assert!(groups()[4].note.is_some());
    }

    #[test]
    fn only_the_two_picture_switches_wear_beta() {
        let beta: Vec<_> = knobs()
            .into_iter()
            .filter(|k| k.beta)
            .map(|k| k.key)
            .collect();
        assert_eq!(beta, vec!["includeImages", "promoteCoverImage"]);
    }

    #[test]
    fn an_indent_is_idle_unless_names_are_indented_and_says_so() {
        let knob = knob_for("cueIndentPct").unwrap();
        let centred = settings(fixtures::defaults());
        assert_eq!(
            idle_reason(&knob, Some(&centred)).as_deref(),
            Some("Only when character names are indented.")
        );
        let mut indented = centred.clone();
        indented.insert("cueAlignment".into(), json!("indented"));
        assert_eq!(idle_reason(&knob, Some(&indented)), None);
        assert_eq!(idle_reason(&knob, None), None);
        assert_eq!(
            idle_reason(&knob_for("fontFamily").unwrap(), Some(&centred)),
            None
        );
    }

    #[test]
    fn the_reason_comes_before_the_help() {
        let mut k = knob_for("cueIndentPct").unwrap();
        k.help = Some("what it is for");
        let notes = notes_for(&k, Some(&settings(fixtures::defaults())));
        assert_eq!(
            notes,
            vec!["Only when character names are indented.", "what it is for"]
        );
        assert!(can_explain(&k));
        assert!(!can_explain(&knob_for("fontFamily").unwrap()));
    }

    #[test]
    fn a_read_out_carries_its_unit_and_says_not_set_in_words() {
        let em = knob_for("elementSpacingEm").unwrap();
        let pct = knob_for("dialogueSideMarginPct").unwrap();
        assert_eq!(display_value(&em, Some(&json!(1))), "1.0 em");
        assert_eq!(display_value(&pct, Some(&json!(20))), "20%");
        assert_eq!(display_value(&pct, None), NOT_SET);
        assert_eq!(display_value(&pct, Some(&json!(null))), NOT_SET);
        assert_eq!(display_value(&pct, Some(&json!("x"))), NOT_SET);
    }

    #[test]
    fn a_moved_control_is_a_value_the_engine_accepts_or_nothing() {
        let pct = knob_for("dialogueSideMarginPct").unwrap();
        assert_eq!(coerce_value(&pct, &json!("25")), Some(json!(25.0)));
        assert_eq!(coerce_value(&pct, &json!(99)), Some(json!(30.0)));
        assert_eq!(coerce_value(&pct, &json!("x")), None);
        let toggle = knob_for("justifyText").unwrap();
        assert_eq!(coerce_value(&toggle, &json!(true)), Some(json!(true)));
        assert_eq!(coerce_value(&toggle, &json!("true")), None);
        let face = knob_for("fontFamily").unwrap();
        assert_eq!(coerce_value(&face, &json!("serif")), Some(json!("serif")));
        assert_eq!(coerce_value(&face, &json!("comic")), None);
    }

    #[test]
    fn changes_accumulate_and_a_failure_gives_them_back() {
        let a = merge_pending(&Settings::new(), "a", json!(1));
        let ab = merge_pending(&a, "b", json!(2));
        assert_eq!(ab.len(), 2);
        let newer = merge_pending(&Settings::new(), "a", json!(9));
        let back = restore_pending(&ab, &newer);
        assert_eq!(back.get("a"), Some(&json!(9)));
        assert_eq!(back.get("b"), Some(&json!(2)));
    }

    #[test]
    fn a_settings_answer_is_checked_knob_by_knob_and_extra_keys_survive() {
        let mut good = fixtures::defaults();
        good["future"] = json!(1);
        let read = settings_from(&json!({"settings": good})).unwrap();
        assert_eq!(read.get("future"), Some(&json!(1)));
        let mut bad = fixtures::defaults();
        bad["fontFamily"] = json!("comic");
        assert!(settings_from(&json!({"settings": bad})).is_none());
        assert!(settings_from(&json!({"settings": []})).is_none());
        assert!(settings_from(&json!({})).is_none());
    }

    #[test]
    fn presets_that_would_half_apply_are_left_out() {
        let answer = json!({"presets": [
            {"id": "kindle", "displayName": "Kindle e-ink", "settings": fixtures::defaults()},
            {"id": "broken", "displayName": "Broken", "settings": {}},
            {"id": "x", "displayName": "", "settings": fixtures::defaults()},
        ], "preset": "kindle"});
        let presets = presets_from(&answer);
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].display_name, "Kindle e-ink");
        assert_eq!(current_preset(&answer).as_deref(), Some("kindle"));
        assert_eq!(current_preset(&json!({"preset": ""})), None);
    }

    #[test]
    fn saved_is_never_said_while_a_change_is_owed() {
        assert_eq!(status_for(Phase::Pending).line, STATUS_PENDING);
        assert_eq!(status_for(Phase::Saved).line, STATUS_SAVED);
        assert_eq!(
            status_for(Phase::Waiting(&["send", "kindle"])).line,
            "Waiting for the Kindle file to finish building…"
        );
        assert_eq!(status_for(Phase::Waiting(&["save"])).line, STATUS_PENDING);
        let failed = status_for(Phase::Failed(Some(" no ")));
        assert_eq!(
            failed,
            Status {
                line: "no".into(),
                bad: true
            }
        );
        assert_eq!(status_for(Phase::Failed(None)).line, NO_MESSAGE);
    }

    #[test]
    fn warnings_are_sentences_in_parentheses() {
        assert_eq!(
            warning_lines(&json!({"warnings": [" a figure is gone ", "", 3]})),
            vec!["(a figure is gone)"]
        );
        assert!(warning_lines(&json!({})).is_empty());
    }

    #[test]
    fn the_fault_screen_is_only_for_a_failure() {
        assert_eq!(empty_pane_mode(true, None), PaneMode::Ready);
        assert_eq!(empty_pane_mode(false, None), PaneMode::Reading);
        assert_eq!(
            empty_pane_mode(
                false,
                Some(&Status {
                    line: "x".into(),
                    bad: true
                })
            ),
            PaneMode::Fault
        );
    }

    #[test]
    fn the_defaults_foot_names_which_defaults_and_offers_reset_only_when_different() {
        let shipped = settings(fixtures::defaults());
        let mut mine = shipped.clone();
        mine.insert("fontFamily".into(), json!("serif"));
        assert_eq!(
            defaults_caption(Some(&shipped), Some(&shipped)),
            "New scripts start from Method Reader’s own defaults."
        );
        assert_eq!(
            defaults_caption(Some(&mine), Some(&shipped)),
            "New scripts start from your own defaults."
        );
        assert!(!can_reset_defaults(Some(&shipped), Some(&shipped)));
        assert!(can_reset_defaults(Some(&mine), Some(&shipped)));
        assert!(!can_reset_defaults(None, Some(&shipped)));
        assert_eq!(defaults_write_args(None), r#"{"formatDefaults":null}"#);
    }

    #[test]
    fn a_defaults_write_outlived_by_its_script_paints_nothing() {
        let answer = json!({"ok": true, "formatDefaults": fixtures::defaults(), "shippedDefaults": fixtures::defaults()});
        assert_eq!(
            defaults_write_outcome(1, 2, &answer, "x"),
            DefaultsOutcome::Stale
        );
        assert!(matches!(
            defaults_write_outcome(2, 2, &answer, "x"),
            DefaultsOutcome::Applied { .. }
        ));
        assert_eq!(
            defaults_write_outcome(2, 2, &json!({"ok": false}), "x"),
            DefaultsOutcome::Refused(NO_MESSAGE.into())
        );
    }

    #[test]
    fn the_keep_choice_paints_only_the_newest_write() {
        let stored = json!({"ok": true, "keepScriptSettings": false});
        assert_eq!(
            keep_write_outcome(1, 1, 2, 2, &stored),
            KeepOutcome {
                stale: false,
                stored: Some(false),
                message: KEEP_SAVED_NOTE.into()
            }
        );
        assert_eq!(
            keep_write_outcome(1, 1, 1, 2, &stored),
            KeepOutcome {
                stale: true,
                stored: Some(false),
                message: String::new()
            }
        );
        assert!(keep_write_outcome(1, 2, 1, 1, &stored).stale);
        let refused = keep_write_outcome(
            1,
            1,
            1,
            1,
            &json!({"ok": false, "error": {"message": "no"}}),
        );
        assert_eq!(refused.message, "no");
        assert_eq!(keep_write_args(true), r#"{"keepScriptSettings":true}"#);
    }

    #[test]
    fn a_switch_write_answers_with_both_switches_or_a_sentence() {
        assert_eq!(
            switch_write_outcome(
                &json!({"ok": true, "animatePage": false, "typewriterSounds": true})
            ),
            Ok((false, true))
        );
        assert_eq!(
            switch_write_outcome(&json!({"ok": true})),
            Err(NO_MESSAGE.into())
        );
    }

    #[test]
    fn only_the_first_group_arrives_open() {
        assert!(group_starts_open(0));
        assert!(!group_starts_open(1));
    }
}
