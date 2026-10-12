//! The release notes the window shows, and `notes-surface.js`'s one rule.
//!
//! `RELEASE` is generated from docs/releases/<version>.md by
//! `cargo xtask build-desktop-notes` (which `cargo xtask bump-version`
//! runs) into notes_release.rs beside this file, the way
//! tools/build-desktop-notes.ts wrote `desktop/ui/notes.js` for the Tauri
//! window. The test below holds it to the release notes on disk.

pub struct Item {
    pub lead: &'static str,
    pub body: &'static str,
}

pub struct Section {
    pub title: &'static str,
    pub items: &'static [Item],
}

pub struct Release {
    pub version: &'static str,
    pub headline: &'static str,
    pub sections: &'static [Section],
}

// The generated `RELEASE` constant.
include!("notes_release.rs");

/// No heading is ever printed with nothing beneath it.
pub fn visible_sections(sections: &'static [Section]) -> Vec<&'static Section> {
    sections.iter().filter(|s| !s.items.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notes_are_the_notes_that_were_written() {
        let path = format!(
            "{}/../../../docs/releases/{}.md",
            env!("CARGO_MANIFEST_DIR"),
            RELEASE.version
        );
        // The Markdown wraps its lines and bolds its leads; the generator
        // joins the one and drops the other.
        let written = std::fs::read_to_string(&path)
            .expect("the release notes for this version")
            .replace("**", "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(written.contains(RELEASE.headline));
        for section in RELEASE.sections {
            assert!(written.contains(section.title), "{}", section.title);
            for item in section.items {
                assert!(written.contains(item.body), "{}", item.body);
            }
        }
    }

    #[test]
    fn the_version_is_the_workspaces() {
        assert_eq!(RELEASE.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn an_empty_section_is_never_drawn() {
        static EMPTY: [Section; 2] = [
            Section {
                title: "a",
                items: &[],
            },
            Section {
                title: "b",
                items: &[Item {
                    lead: "",
                    body: "x",
                }],
            },
        ];
        let shown: Vec<_> = visible_sections(&EMPTY).iter().map(|s| s.title).collect();
        assert_eq!(shown, vec!["b"]);
    }
}
