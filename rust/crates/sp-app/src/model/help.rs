//! The Help menu: what it holds on each platform, the pages it opens, the
//! keyboard shortcuts it lists, and the licence texts it shows (the suite's
//! standard, SUITE-STANDARDS.md "The Help menu"). On a Mac the items are
//! the menu bar's Help menu (src/app/menu.rs), with About in the app menu as
//! macOS has it; on Windows and Linux they are the window's own Help menu,
//! at the foot of the page beside the version stamp (src/app/help.rs).

use super::convert::Platform;
use super::names::PRODUCT;

/// One thing the Help menu can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HelpItem {
    About,
    UserGuide,
    Shortcuts,
    WhatsNew,
    CheckForUpdates,
    /// The "Check for Updates Automatically" switch.
    AutomaticUpdates,
    ReportBug,
    Licenses,
    Suite,
}

impl HelpItem {
    /// What the menu says.
    pub fn label(self) -> String {
        match self {
            HelpItem::About => format!("About {PRODUCT}"),
            HelpItem::UserGuide => "User Guide".into(),
            HelpItem::Shortcuts => "Keyboard Shortcuts".into(),
            HelpItem::WhatsNew => "What's New".into(),
            HelpItem::CheckForUpdates => super::update::CHECK_MENU.into(),
            HelpItem::AutomaticUpdates => super::update::AUTO_LABEL.into(),
            HelpItem::ReportBug => "Report a Bug".into(),
            HelpItem::Licenses => "Licenses and Notices".into(),
            HelpItem::Suite => "Method Production Suite".into(),
        }
    }
}

/// The Help menu, in order, in groups (a separator between groups). On a
/// Mac, About is in the app menu instead, where macOS puts it.
pub fn groups(platform: Platform) -> Vec<Vec<HelpItem>> {
    use HelpItem::*;
    let mut first = vec![UserGuide, Shortcuts, WhatsNew];
    if platform != Platform::Mac {
        first.insert(0, About);
    }
    vec![
        first,
        vec![CheckForUpdates, AutomaticUpdates],
        vec![ReportBug, Licenses],
        vec![Suite],
    ]
}

/// Method Reader's website.
pub const WEBSITE: &str = "https://methodreader.com";
/// What About shows the website as.
pub const WEBSITE_WORDS: &str = "methodreader.com";
/// The how-to: the README's "What it does", on GitHub.
pub const USER_GUIDE: &str =
    "https://github.com/method-production-suite/method-reader#what-it-does";
/// The suite's own site.
pub const SUITE: &str = "https://methodproduction.com";
/// Every release, for a computer that does not update itself.
pub const RELEASES: &str = "https://github.com/method-production-suite/method-reader/releases";

/// The copyright line every page that states one carries.
pub const COPYRIGHT: &str =
    "Copyright (c) 2026 Method Production Suite Team and the Method Reader contributors";

/// What About says about the licence, in plain words.
pub const LICENCE_LINE: &str = "Method Reader's own code is free and open source, under the MIT License or the Apache License 2.0, at your option. Its AZW3 writer is ported from calibre and licensed under the GNU GPL version 3, so the app as a whole is distributed under GPL-3.0.";

/// What About says the app is.
pub const ABOUT_LINE: &str = "Turns a screenplay PDF into an e-book for your e-reader, and into a script you can edit again.";

/// One licence or notice, as the Licenses and Notices window shows it.
#[derive(Clone, Copy, Debug)]
pub struct Notice {
    pub title: &'static str,
    pub text: &'static str,
}

/// The licence texts and the third-party notices, embedded when the app is
/// built, so they travel inside the program and need no file at run time.
pub const NOTICES: [Notice; 4] = [
    Notice {
        title: "MIT License",
        text: include_str!("../../../../../LICENSE-MIT"),
    },
    Notice {
        title: "Apache License 2.0",
        text: include_str!("../../../../../LICENSE-APACHE"),
    },
    Notice {
        title: "GNU GPL 3.0 (the AZW3 writer)",
        text: include_str!("../../../sp-azw3/LICENSE-GPL-3.0"),
    },
    Notice {
        title: "Third-party notices",
        text: include_str!("../../../../../THIRD-PARTY-NOTICES.md"),
    },
];

/// The keyboard, as the window answers it: the keys, then what they do.
pub fn shortcuts(platform: Platform) -> Vec<(String, &'static str)> {
    let (cmd, shift) = if platform == Platform::Mac {
        ("⌘", "⇧⌘")
    } else {
        ("Ctrl+", "Ctrl+Shift+")
    };
    let mut rows: Vec<(String, &'static str)> = vec![
        (
            super::convert::shortcut_label(platform).to_string(),
            "Choose a screenplay to convert",
        ),
        (
            "Tab, Shift+Tab".into(),
            "Move to the next or the last control",
        ),
        (
            "Space, Return".into(),
            "Press the control that has the keyboard",
        ),
        (
            "Left, Right, Home, End".into(),
            "On the tabs: move between Convert, Read and Settings",
        ),
        (
            "Arrows, Home, End".into(),
            "In a group of choices: move between them",
        ),
        (
            "Up, Down, Page Up, Page Down, Space, Home, End".into(),
            "On the Read page's book: scroll it",
        ),
        (
            "Up, Down, Home, End".into(),
            "In the Read page's list of scenes: move between scenes",
        ),
        ("Escape".into(), "Close the sheet or the menu that is open"),
    ];
    if platform == Platform::Mac {
        rows.extend([
            (format!("{cmd}Z, {shift}Z"), "Undo, redo, in a text field"),
            (format!("{cmd}X, {cmd}C, {cmd}V"), "Cut, copy, paste"),
            (format!("{cmd}A"), "Select all"),
            (format!("{cmd}H"), "Hide Method Reader"),
            (format!("{cmd}M"), "Minimize the window"),
            (format!("{cmd}W, {cmd}Q"), "Close the window, quit"),
        ]);
    } else {
        rows.extend([
            (format!("{cmd}Z, {shift}Z"), "Undo, redo, in a text field"),
            (format!("{cmd}X, {cmd}C, {cmd}V"), "Cut, copy, paste"),
            (format!("{cmd}A"), "Select all"),
        ]);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mac_keeps_about_in_the_app_menu_and_everywhere_else_it_leads_help() {
        let mac: Vec<HelpItem> = groups(Platform::Mac).concat();
        assert!(!mac.contains(&HelpItem::About));
        for p in [Platform::Windows, Platform::Linux] {
            let items = groups(p).concat();
            assert_eq!(items[0], HelpItem::About);
            assert_eq!(items.len(), mac.len() + 1);
        }
        assert_eq!(
            mac,
            [
                HelpItem::UserGuide,
                HelpItem::Shortcuts,
                HelpItem::WhatsNew,
                HelpItem::CheckForUpdates,
                HelpItem::AutomaticUpdates,
                HelpItem::ReportBug,
                HelpItem::Licenses,
                HelpItem::Suite,
            ]
        );
    }

    #[test]
    fn the_labels_are_the_suite_s() {
        assert_eq!(HelpItem::About.label(), "About Method Reader");
        assert_eq!(HelpItem::CheckForUpdates.label(), "Check for Updates…");
        assert_eq!(
            HelpItem::AutomaticUpdates.label(),
            "Check for Updates Automatically"
        );
        assert_eq!(HelpItem::Licenses.label(), "Licenses and Notices");
        assert_eq!(HelpItem::Suite.label(), "Method Production Suite");
    }

    #[test]
    fn the_notices_are_the_repository_s_own_files_built_in() {
        assert!(NOTICES[0].text.starts_with("MIT License"));
        assert!(NOTICES[0].text.contains(COPYRIGHT));
        assert!(NOTICES[1].text.contains("Apache License"));
        assert!(NOTICES[2].text.contains("GNU GENERAL PUBLIC LICENSE"));
        assert!(NOTICES[3].text.starts_with("# Third-party notices"));
    }

    #[test]
    fn the_shortcuts_name_the_platform_s_own_keys() {
        let mac = shortcuts(Platform::Mac);
        assert_eq!(mac[0].0, "⌘O");
        assert!(mac.iter().any(|(k, _)| k == "⌘A"));
        let linux = shortcuts(Platform::Linux);
        assert_eq!(linux[0].0, "Ctrl+O");
        assert!(linux.iter().all(|(k, _)| !k.contains('⌘')));
    }
}
