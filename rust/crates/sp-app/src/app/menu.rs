//! The Mac's menu bar, as Tauri's default menu gave the window: the app
//! menu (About, Services, Hide, Hide Others, Show All, Quit), Edit (Undo,
//! Redo, Cut, Copy, Paste, Select All) and Window (Minimize, Zoom, Close);
//! and from 0.13 Help, the suite's Help menu (model/help.rs). About and
//! every Help item open the window's own sheets (src/app/help.rs), so a screen
//! reader meets them as it meets the rest of the window.
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
    use crate::model::convert::Platform;
    use crate::model::help::{HelpItem, groups};
    use crate::model::names::PRODUCT;
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

    use super::{EditAction, edit_events};

    /// The menu bar, kept alive for the window's life, its Edit items, and
    /// the items that open the window's Help sheets.
    pub struct Menus {
        _menu: Menu,
        edit: Vec<(MenuId, EditAction)>,
        help: Vec<(MenuId, HelpItem)>,
        /// Check for Updates Automatically, kept to the stored switch.
        automatic: CheckMenuItem,
    }

    impl Menus {
        /// Build the bar and give it to the app. Call once, on the main
        /// thread, after the window exists.
        pub fn install() -> Option<Self> {
            let menu = Menu::new();
            let app = Submenu::new(PRODUCT, true);
            // The window's own About sheet (version, website, licence),
            // named here rather than read from the bundle, so a development
            // build and a bundle still under its old name (Screepub.app,
            // until its first update) both say Method Reader.
            let about = MenuItem::new(HelpItem::About.label(), true, None);
            app.append_items(&[
                &about,
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
            let help_menu = Submenu::new("Help", true);
            let mut help = vec![(about.id().clone(), HelpItem::About)];
            let automatic =
                CheckMenuItem::new(HelpItem::AutomaticUpdates.label(), true, true, None);
            for (i, group) in groups(Platform::Mac).into_iter().enumerate() {
                if i > 0 {
                    help_menu.append(&PredefinedMenuItem::separator()).ok()?;
                }
                for item in group {
                    match item {
                        HelpItem::AutomaticUpdates => {
                            help_menu.append(&automatic).ok()?;
                            help.push((automatic.id().clone(), item));
                        }
                        _ => {
                            let entry = MenuItem::new(item.label(), true, None);
                            help_menu.append(&entry).ok()?;
                            help.push((entry.id().clone(), item));
                        }
                    }
                }
            }
            menu.append_items(&[&app, &edit_menu, &window, &help_menu])
                .ok()?;
            menu.init_for_nsapp();
            window.set_as_windows_menu_for_nsapp();
            help_menu.set_as_help_menu_for_nsapp();
            let edit = vec![
                (undo.id().clone(), EditAction::Undo),
                (redo.id().clone(), EditAction::Redo),
                (cut.id().clone(), EditAction::Cut),
                (copy.id().clone(), EditAction::Copy),
                (paste.id().clone(), EditAction::Paste),
                (all.id().clone(), EditAction::SelectAll),
            ];
            Some(Self {
                _menu: menu,
                edit,
                help,
                automatic,
            })
        }

        /// Keep the switch to what is stored (it can change in the update
        /// sheet too), and to whether this copy can update itself at all.
        /// Check for Updates… stays enabled: where this copy cannot update
        /// itself it says where releases are.
        pub fn sync(&self, usable: bool, automatic: bool) {
            if self.automatic.is_checked() != (usable && automatic) {
                self.automatic.set_checked(usable && automatic);
            }
            if self.automatic.is_enabled() != usable {
                self.automatic.set_enabled(usable);
            }
        }

        /// Pass every Edit item chosen since the last frame on to egui, and
        /// hand back the Help items chosen, for the window to act on.
        pub fn pump(&self, ctx: &egui::Context) -> Vec<HelpItem> {
            let mut chosen = Vec::new();
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if let Some((_, item)) = self.help.iter().find(|(id, _)| *id == event.id) {
                    chosen.push(*item);
                    continue;
                }
                let Some((_, action)) = self.edit.iter().find(|(id, _)| *id == event.id) else {
                    continue;
                };
                let clipboard = (*action == EditAction::Paste)
                    .then(|| arboard::Clipboard::new().ok()?.get_text().ok())
                    .flatten();
                let events = edit_events(*action, clipboard);
                ctx.input_mut(|i| i.events.extend(events));
            }
            chosen
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
