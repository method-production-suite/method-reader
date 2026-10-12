//! Ports tests/torture-coverage.test.ts, and checks `device-checklist`.
//!
//! The manifest (tools/torture-manifest.json) is the single source of truth
//! for what the torture fixture proves. This exists so the answer to "does
//! the fixture still cover the registry?" is enforced rather than
//! remembered: a new registry entry with no decision recorded against it
//! fails the suite. It does NOT require covered: true. "covered": false
//! with a reason IS a decision. Silence is not.

use std::collections::HashSet;

use xtask::device_checklist::{self, Row};
use xtask::paths;

fn manifest() -> Vec<Row> {
    device_checklist::load().expect("the manifest parses")
}

/// Entry numbers, read from the registry's own `### <n>.` headings.
fn registry_entries() -> Vec<String> {
    let md = std::fs::read_to_string(paths::repo_root().join("docs/formatting-options-log.md"))
        .expect("the registry reads");
    md.lines()
        .filter_map(|l| l.strip_prefix("### "))
        .filter_map(|rest| {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            if digits == 0 {
                return None;
            }
            let after = &rest[digits..];
            let (letter, tail) = match after.chars().next() {
                Some(c) if c.is_ascii_lowercase() => (1, &after[1..]),
                _ => (0, after),
            };
            tail.starts_with('.')
                .then(|| rest[..digits + letter].to_string())
        })
        .collect()
}

#[test]
fn the_registry_is_non_empty_and_parses() {
    // Guards the whole file: if the heading shape ever changes, every other
    // assertion here would pass vacuously against an empty list.
    assert!(registry_entries().len() > 20);
}

#[test]
fn every_registry_entry_has_a_row_covered_or_explicitly_not() {
    let rows: HashSet<String> = manifest().into_iter().map(|r| r.entry).collect();
    let missing: Vec<String> = registry_entries()
        .into_iter()
        .filter(|e| !rows.contains(e))
        .collect();
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn no_row_names_an_entry_the_registry_does_not_have() {
    let entries: HashSet<String> = registry_entries().into_iter().collect();
    let orphans: Vec<String> = manifest()
        .into_iter()
        .map(|r| r.entry)
        .filter(|e| !entries.contains(e))
        .collect();
    assert_eq!(orphans, Vec::<String>::new());
}

#[test]
fn no_entry_is_listed_twice() {
    let seen: Vec<String> = manifest().into_iter().map(|r| r.entry).collect();
    let unique: HashSet<&String> = seen.iter().collect();
    assert_eq!(seen.len(), unique.len());
}

#[test]
fn covered_rows_say_where_which_side_and_how() {
    for row in manifest().iter().filter(|r| r.covered) {
        assert!(
            row.page_value.as_ref().is_some_and(|v| v.is_number()),
            "entry {} page",
            row.entry
        );
        assert!(
            matches!(row.side.as_deref(), Some("source" | "device" | "both")),
            "entry {} side",
            row.entry
        );
        assert!(
            row.how.as_deref().is_some_and(|h| !h.is_empty()),
            "entry {} how",
            row.entry
        );
    }
}

#[test]
fn uncovered_rows_say_why_and_claim_nothing_else() {
    for row in manifest().iter().filter(|r| !r.covered) {
        assert!(
            row.why.as_deref().is_some_and(|w| !w.is_empty()),
            "entry {} why",
            row.entry
        );
        assert!(
            !row.has_page_key,
            "entry {} should not claim a page",
            row.entry
        );
    }
}

// device-checklist had no test of its own; these pin what it prints to the
// manifest, so a change to either shows.

#[test]
fn the_checklist_lists_every_device_entry_and_every_gap() {
    let rows = manifest();
    let out = device_checklist::render(&rows);
    let device = rows
        .iter()
        .filter(|r| r.covered && matches!(r.side.as_deref(), Some("device" | "both")))
        .count();
    let gaps = rows.iter().filter(|r| !r.covered).count();
    assert_eq!(out.matches("\n- [ ] **#").count(), device);
    for r in rows.iter().filter(|r| !r.covered) {
        assert!(out.contains(&format!("- **#{}, {}.**", r.entry, r.title)));
    }
    assert!(out.contains(&format!(
        "{device} device-side entries, {gaps} declared gaps, "
    )));
    assert!(out.starts_with("# Device pass: THE PROOF SHEET\n"));
}

#[test]
fn the_checklist_runs_in_page_order() {
    let rows = manifest();
    let out = device_checklist::render(&rows);
    let sheets: Vec<u32> = out
        .lines()
        .filter(|l| l.starts_with("- [ ] **#"))
        .map(|l| {
            let at = l.rfind("(sheet ").expect("a sheet") + 7;
            l[at..l.len() - 1].parse().expect("a sheet number")
        })
        .collect();
    let mut sorted = sheets.clone();
    sorted.sort_unstable();
    assert_eq!(sheets, sorted);
}

#[test]
fn every_device_check_is_a_box_to_tick() {
    let rows = manifest();
    let out = device_checklist::render(&rows);
    for r in &rows {
        for c in &r.device_checks {
            assert!(out.contains(&format!("- [ ] {c}")), "entry {}", r.entry);
        }
    }
}

#[test]
fn the_command_prints_the_checklist_and_succeeds() {
    let exe = env!("CARGO_BIN_EXE_xtask");
    let out = std::process::Command::new(exe)
        .arg("device-checklist")
        .output()
        .expect("xtask runs");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert_eq!(text, format!("{}\n", device_checklist::render(&manifest())));
}
