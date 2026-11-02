//! The tab bar and the stamp (was `desktop/ui/frame.js`).

/// The surfaces the bar switches between. Notes is not one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    Convert,
    /// Every script in the library folder (src/model/library.rs).
    Library,
    Read,
    Tune,
}

impl Surface {
    pub const ALL: [Surface; 4] = [
        Surface::Convert,
        Surface::Library,
        Surface::Read,
        Surface::Tune,
    ];

    /// What the tab says. The ids stay `read` and `tune`; the labels are
    /// Preview and Settings.
    pub fn label(self) -> &'static str {
        match self {
            Self::Convert => "Convert",
            Self::Library => "Library",
            Self::Read => "Preview",
            Self::Tune => "Settings",
        }
    }

    /// The surfaces that have nothing to show until a script is open.
    pub fn needs_script(self) -> bool {
        !matches!(self, Self::Convert | Self::Library)
    }

    /// How many staggers late this tab arrives when a script opens: the
    /// tabs that arrive, in order, the second one a stagger behind the
    /// first (style.css tab-arrive). None for a tab that is always there.
    pub fn arrival_place(self) -> Option<usize> {
        Self::ALL
            .iter()
            .filter(|s| s.needs_script())
            .position(|s| *s == self)
    }
}

/// The revision stamp's words.
pub fn rev_label(version: &str) -> String {
    format!("rev {version}")
}

/// Whether a tab is on the bar, and whether it is the keyboard's one stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presence {
    pub shown: bool,
    pub tab_stop: bool,
}

pub fn tab_presence(available: bool, selected: bool) -> Presence {
    if !available {
        return Presence {
            shown: false,
            tab_stop: false,
        };
    }
    Presence {
        shown: true,
        tab_stop: selected,
    }
}

/// The tablist's keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKey {
    Right,
    Left,
    Home,
    End,
}

/// Where a key moves the selection from `current`, skipping absent tabs;
/// Left and Right wrap. None when nowhere else is available.
pub fn tab_step(
    key: TabKey,
    current: Surface,
    available: impl Fn(Surface) -> bool,
) -> Option<Surface> {
    let all = Surface::ALL;
    let n = all.len();
    let from = all.iter().position(|s| *s == current).unwrap_or(0);
    let order: Vec<Surface> = match key {
        TabKey::Right => (1..n).map(|hop| all[(from + hop) % n]).collect(),
        TabKey::Left => (1..n).map(|hop| all[(from + n * n - hop) % n]).collect(),
        TabKey::Home => all.to_vec(),
        TabKey::End => all.iter().rev().copied().collect(),
    };
    order.into_iter().find(|s| available(*s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_is_convert_library_preview_settings() {
        let labels: Vec<_> = Surface::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(labels, vec!["Convert", "Library", "Preview", "Settings"]);
        assert!(!Surface::Convert.needs_script());
        assert!(
            !Surface::Library.needs_script(),
            "there with no script open"
        );
        assert!(Surface::Read.needs_script() && Surface::Tune.needs_script());
    }

    #[test]
    fn preview_arrives_first_and_settings_a_stagger_after() {
        assert_eq!(Surface::Convert.arrival_place(), None);
        assert_eq!(Surface::Library.arrival_place(), None);
        assert_eq!(Surface::Read.arrival_place(), Some(0));
        assert_eq!(Surface::Tune.arrival_place(), Some(1));
    }

    #[test]
    fn the_stamp_says_rev() {
        assert_eq!(rev_label("0.11.1"), "rev 0.11.1");
    }

    #[test]
    fn a_surface_with_nothing_behind_it_is_absent_not_dimmed() {
        assert_eq!(
            tab_presence(false, true),
            Presence {
                shown: false,
                tab_stop: false
            }
        );
        assert_eq!(
            tab_presence(true, false),
            Presence {
                shown: true,
                tab_stop: false
            }
        );
        assert_eq!(
            tab_presence(true, true),
            Presence {
                shown: true,
                tab_stop: true
            }
        );
    }

    #[test]
    fn the_arrows_wrap_and_skip_absent_tabs() {
        let all = |_| true;
        assert_eq!(
            tab_step(TabKey::Right, Surface::Tune, all),
            Some(Surface::Convert)
        );
        assert_eq!(
            tab_step(TabKey::Left, Surface::Convert, all),
            Some(Surface::Tune)
        );
        assert_eq!(
            tab_step(TabKey::Home, Surface::Tune, all),
            Some(Surface::Convert)
        );
        assert_eq!(
            tab_step(TabKey::End, Surface::Convert, all),
            Some(Surface::Tune)
        );
        assert_eq!(
            tab_step(TabKey::Right, Surface::Convert, all),
            Some(Surface::Library)
        );
        assert_eq!(
            tab_step(TabKey::Left, Surface::Read, all),
            Some(Surface::Library)
        );
        // No script open: Convert and Library, and the arrows wrap between them.
        let no_script = |s: Surface| !s.needs_script();
        assert_eq!(
            tab_step(TabKey::Right, Surface::Library, no_script),
            Some(Surface::Convert)
        );
        assert_eq!(
            tab_step(TabKey::Left, Surface::Convert, no_script),
            Some(Surface::Library)
        );
        assert_eq!(
            tab_step(TabKey::End, Surface::Convert, no_script),
            Some(Surface::Library)
        );
        let only_convert = |s| s == Surface::Convert;
        assert_eq!(
            tab_step(TabKey::Right, Surface::Convert, only_convert),
            None
        );
        assert_eq!(
            tab_step(TabKey::End, Surface::Convert, only_convert),
            Some(Surface::Convert)
        );
    }
}
