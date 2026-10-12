//! The release notes the window shows: `desktop/ui/notes.js` (generated
//! from docs/releases/<version>.md by tools/build-desktop-notes.ts) and
//! `notes-surface.js`'s one rule.
//!
//! Copied from the generated module for 0.11.1. The xtask port of
//! build-desktop-notes.ts generates this file in its place; the test below
//! holds it to the release notes on disk until then.

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

pub const RELEASE: Release = Release {
    version: "0.11.1",
    headline: "Your Kindle file is ready almost instantly, with nothing else to install.",
    sections: &[
        Section {
            title: "Kindles over USB, faster",
            items: &[
                Item {
                    lead: "Screepub makes the Kindle file itself.",
                    body: "Copying to a Kindle over USB used to need Calibre, and waited on it to start every time. Now Screepub writes the file in a fraction of a second, with nothing installed. It is marked Beta.",
                },
                Item {
                    lead: "Calibre is only for the best-layout file.",
                    body: "With Calibre, its KFX plugin and Kindle Previewer installed, modern Kindles still get KFX. Emailing to Kindle needs no tools at all.",
                },
            ],
        },
        Section {
            title: "Good to know",
            items: &[
                Item {
                    lead: "",
                    body: "The new Kindle file reads back whole in two independent e-book readers, but hasn't been opened on a real Kindle yet. Tell us how it goes.",
                },
                Item {
                    lead: "",
                    body: "Kindles from before 2012 can't read it. Use Save a file, then MOBI.",
                },
                Item {
                    lead: "",
                    body: "Kindle files Calibre made for your books are rebuilt once, the next time you send each one.",
                },
            ],
        },
    ],
};

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
