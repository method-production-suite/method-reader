//! The Mac's menu bar, as Tauri's default menu gave the window: the app
//! menu (About, Services, Hide, Hide Others, Show All, Quit), Edit (Undo,
//! Redo, Cut, Copy, Paste, Select All) and Window (Minimize, Zoom, Close).
//!
//! The Edit items carry their usual shortcuts, which macOS then hands to the
//! menu and not to the window; so each one is passed on to egui as the
//! keystroke or clipboard event it stands for, and the device-request field
//! copies and pastes exactly as before. Windows and Linux had no menu.

/// What an Edit item stands for in egui's own terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// The egui events an Edit item becomes, given the clipboard's text for a
/// paste.
pub fn edit_events(action: EditAction, clipboard: Option<String>) -> Vec<egui::Event> {
    let key = |key: egui::Key, shift: bool| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers {
            shift,
            ..egui::Modifiers::COMMAND
        },
    };
    match action {
        EditAction::Undo => vec![key(egui::Key::Z, false)],
        EditAction::Redo => vec![key(egui::Key::Z, true)],
        EditAction::Cut => vec![egui::Event::Cut],
        EditAction::Copy => vec![egui::Event::Copy],
        EditAction::Paste => clipboard.map(egui::Event::Paste).into_iter().collect(),
        EditAction::SelectAll => vec![key(egui::Key::A, false)],
    }
}

#[cfg(target_os = "macos")]
pub use mac::Menus;

#[cfg(target_os = "macos")]
mod mac {
    use crate::model::names::PRODUCT;
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{AboutMetadata, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

    use super::{EditAction, edit_events};

    /// The menu bar, kept alive for the window's life, and its Edit items.
    pub struct Menus {
        _menu: Menu,
        edit: Vec<(MenuId, EditAction)>,
    }

    impl Menus {
        /// Build the bar and give it to the app. Call once, on the main
        /// thread, after the window exists.
        pub fn install() -> Option<Self> {
            let menu = Menu::new();
            let app = Submenu::new(PRODUCT, true);
            app.append_items(&[
                // Named here rather than read from the bundle, so a
                // development build and a bundle still under its old
                // name (Screepub.app, until its first update) both say
                // Method Reader.
                &PredefinedMenuItem::about(
                    Some(&format!("About {PRODUCT}")),
                    Some(AboutMetadata {
                        name: Some(PRODUCT.into()),
                        version: Some(env!("CARGO_PKG_VERSION").into()),
                        ..Default::default()
                    }),
                ),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ])
            .ok()?;
            let cmd = Modifiers::META;
            let item = |words: &str, mods: Modifiers, code: Code| {
                MenuItem::new(words, true, Some(Accelerator::new(mods, code)))
            };
            let undo = item("Undo", cmd, Code::KeyZ);
            let redo = item("Redo", cmd | Modifiers::SHIFT, Code::KeyZ);
            let cut = item("Cut", cmd, Code::KeyX);
            let copy = item("Copy", cmd, Code::KeyC);
            let paste = item("Paste", cmd, Code::KeyV);
            let all = item("Select All", cmd, Code::KeyA);
            let edit_menu = Submenu::new("Edit", true);
            edit_menu
                .append_items(&[
                    &undo,
                    &redo,
                    &PredefinedMenuItem::separator(),
                    &cut,
                    &copy,
                    &paste,
                    &all,
                ])
                .ok()?;
            let window = Submenu::new("Window", true);
            window
                .append_items(&[
                    &PredefinedMenuItem::minimize(None),
                    &PredefinedMenuItem::maximize(Some("Zoom")),
                    &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::close_window(None),
                ])
                .ok()?;
            menu.append_items(&[&app, &edit_menu, &window]).ok()?;
            menu.init_for_nsapp();
            window.set_as_windows_menu_for_nsapp();
            let edit = vec![
                (undo.id().clone(), EditAction::Undo),
                (redo.id().clone(), EditAction::Redo),
                (cut.id().clone(), EditAction::Cut),
                (copy.id().clone(), EditAction::Copy),
                (paste.id().clone(), EditAction::Paste),
                (all.id().clone(), EditAction::SelectAll),
            ];
            Some(Self { _menu: menu, edit })
        }

        /// Pass every Edit item chosen since the last frame on to egui.
        pub fn pump(&self, ctx: &egui::Context) {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                let Some((_, action)) = self.edit.iter().find(|(id, _)| *id == event.id) else {
                    continue;
                };
                let clipboard = (*action == EditAction::Paste)
                    .then(|| arboard::Clipboard::new().ok()?.get_text().ok())
                    .flatten();
                let events = edit_events(*action, clipboard);
                ctx.input_mut(|i| i.events.extend(events));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_edit_item_becomes_what_its_shortcut_would_have_been() {
        assert_eq!(edit_events(EditAction::Copy, None), vec![egui::Event::Copy]);
        assert_eq!(edit_events(EditAction::Cut, None), vec![egui::Event::Cut]);
        assert_eq!(
            edit_events(EditAction::Paste, Some("Boox".into())),
            vec![egui::Event::Paste("Boox".into())]
        );
        assert!(
            edit_events(EditAction::Paste, None).is_empty(),
            "nothing on the clipboard pastes nothing"
        );
        let egui::Event::Key { key, modifiers, .. } =
            edit_events(EditAction::Redo, None)[0].clone()
        else {
            panic!("a keystroke")
        };
        assert_eq!(key, egui::Key::Z);
        assert!(modifiers.command && modifiers.shift);
    }
}
