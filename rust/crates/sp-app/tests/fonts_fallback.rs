//! The window on a machine whose UI face lacks a character the chrome
//! draws. Windows' Segoe UI has no U+25B8, and egui finds no font for it
//! anywhere behind the system face, so the full run's Windows tests stopped
//! on the Settings groups' disclosure triangle (2026-11-16), and a reader
//! there saw a box. This takes the system face out, leaving the faces that
//! ship in the binary, which is what every machine can count on, and walks
//! the screens that draw chrome. The harness panics on any missing glyph.

mod common;

use common::*;
use egui::FontFamily;
use egui_kittest::kittest::Queryable;
use sp_app::model::convert::Platform;

/// The window's own font set with the system face taken out of every family.
fn shipped_faces_only(h: &mut H) {
    let mut defs = sp_app::fonts::definitions();
    for list in defs.families.values_mut() {
        list.retain(|f| !f.starts_with("system-ui"));
    }
    defs.font_data
        .retain(|name, _| !name.starts_with("system-ui"));
    assert!(
        defs.families
            .get(&FontFamily::Name(sp_app::theme::CHROME.into()))
            .is_some_and(|l| !l.is_empty()),
        "the chrome keeps its bundled faces"
    );
    h.ctx.set_fonts(defs);
    steps(h);
}

#[test]
fn the_settings_groups_draw_with_only_the_faces_that_ship() {
    let w = world();
    let mut h = harness_sized(&w, Platform::Windows, [1280.0, 4000.0]);
    shipped_faces_only(&mut h);
    convert(&w, &mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Tab, "Settings")
        .click();
    steps(&mut h);
    for group in [
        "Dialogue",
        "The text",
        "What the book carries",
        "Taken from the PDF",
    ] {
        h.get_by_role_and_label(egui::accesskit::Role::DisclosureTriangle, group)
            .click_accesskit();
        steps(&mut h);
    }
}
