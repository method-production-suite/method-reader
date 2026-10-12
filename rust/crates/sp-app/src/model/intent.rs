//! Read it | Edit it: what the send panel leads with. New behavior in the
//! Rust window, not parity: the script-for-editing files (FDX, Fountain)
//! were only reachable under Other ways > Save a file, after a reader was
//! chosen. The choice is remembered app-wide (`panelIntent` in app
//! settings), like the reader device.

use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Intent {
    #[default]
    Read,
    Edit,
}

impl Intent {
    pub fn label(self) -> &'static str {
        match self {
            Intent::Read => READ_IT,
            Intent::Edit => EDIT_IT,
        }
    }

    /// The word app settings store.
    pub fn stored(self) -> &'static str {
        match self {
            Intent::Read => "read",
            Intent::Edit => "edit",
        }
    }

    pub fn other(self) -> Intent {
        match self {
            Intent::Read => Intent::Edit,
            Intent::Edit => Intent::Read,
        }
    }

    /// Where the brass knob rests: 0 under Read it, 1 under Edit it.
    pub fn knob(self) -> f32 {
        match self {
            Intent::Read => 0.0,
            Intent::Edit => 1.0,
        }
    }
}

pub const READ_IT: &str = "Read it";
pub const EDIT_IT: &str = "Edit it";
/// The control's accessible name.
pub const CHOICE_LABEL: &str = "Read it or edit it";
pub const EDIT_LINE: &str = "Get the script back to edit, rebuilt from the PDF’s pages.";

/// One of Edit it's two cards: the file type it saves, its name and the
/// line under it, and whether it wears the Beta badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditCard {
    pub kind: &'static str,
    pub name: &'static str,
    pub sub: &'static str,
    pub beta: bool,
}

pub const EDIT_CARDS: [EditCard; 2] = [
    EditCard {
        kind: "fdx",
        name: "Final Draft",
        sub: "FDX, also for most screenwriting apps",
        beta: true,
    },
    EditCard {
        kind: "fountain",
        name: "Highland, Fade In and others",
        sub: "Fountain, plain text",
        beta: false,
    },
];

/// The stored choice in an app-settings answer; anything else is Read it.
pub fn intent_from(answer: &Value) -> Intent {
    if !super::json::ok(answer) {
        return Intent::Read;
    }
    match answer.get("panelIntent").and_then(Value::as_str) {
        Some("edit") => Intent::Edit,
        _ => Intent::Read,
    }
}

/// The keys that move the choice: Left/Right (and Up/Down) flip it, Home
/// picks Read it, End picks Edit it. None: the key does nothing.
pub fn step(current: Intent, key: egui::Key) -> Option<Intent> {
    use egui::Key::*;
    let next = match key {
        ArrowLeft | ArrowRight | ArrowUp | ArrowDown => current.other(),
        Home => Intent::Read,
        End => Intent::Edit,
        _ => return None,
    };
    (next != current).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_stored_choice_is_read_unless_it_says_edit() {
        assert_eq!(intent_from(&json!({"ok": true})), Intent::Read);
        assert_eq!(
            intent_from(&json!({"ok": true, "panelIntent": "edit"})),
            Intent::Edit
        );
        assert_eq!(
            intent_from(&json!({"ok": true, "panelIntent": "sideways"})),
            Intent::Read
        );
        assert_eq!(
            intent_from(&json!({"ok": false, "panelIntent": "edit"})),
            Intent::Read
        );
    }

    #[test]
    fn the_arrows_flip_it_and_home_and_end_pick_an_end() {
        assert_eq!(
            step(Intent::Read, egui::Key::ArrowRight),
            Some(Intent::Edit)
        );
        assert_eq!(step(Intent::Edit, egui::Key::ArrowLeft), Some(Intent::Read));
        assert_eq!(step(Intent::Read, egui::Key::Home), None);
        assert_eq!(step(Intent::Read, egui::Key::End), Some(Intent::Edit));
        assert_eq!(step(Intent::Read, egui::Key::A), None);
    }

    #[test]
    fn the_cards_are_the_editing_types() {
        assert_eq!(EDIT_CARDS.map(|c| c.kind), ["fdx", "fountain"]);
    }
}
