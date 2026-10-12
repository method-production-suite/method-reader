//! Hand-built parser elements, the `el({...})` of the TypeScript tests.

#![allow(dead_code)]

use sp_model::{
    ElementType, FamilyBucket, Fmt, ImageRef, ParsedScreenplay, ScreenplayElement, SizeStep,
};

#[allow(unused_imports)]
pub use sp_model::ElementType::{
    Action, Character, Dialogue, Image, MiniSlug, PageNumber, Parenthetical, Scene, Transition,
};

/// An element on page 1, readable, not on the title page.
pub fn el(kind: ElementType, text: &str) -> ScreenplayElement {
    ScreenplayElement {
        kind,
        text: text.to_owned(),
        id: "t".to_owned(),
        page_num: 1,
        is_title_page: false,
        is_readable: true,
        character: None,
        base_character: None,
        age_modifier: None,
        age_value: None,
        scene_number: None,
        styled_text: None,
        dual_right: None,
        fmt: None,
        img: None,
        y: None,
    }
}

/// A cue or speech line belonging to `who`.
pub fn said(kind: ElementType, text: &str, who: &str) -> ScreenplayElement {
    el(kind, text).who(who)
}

/// An image element for `file` at 71% unless changed.
pub fn img(file: &str) -> ScreenplayElement {
    let mut e = el(Image, file);
    e.img = Some(ImageRef {
        file: file.to_owned(),
        width_pct: 71.0,
        full_page: false,
    });
    e
}

pub fn screenplay(elements: Vec<ScreenplayElement>) -> ParsedScreenplay {
    ParsedScreenplay {
        elements,
        characters: Vec::new(),
        scenes: Vec::new(),
        page_count: 1,
    }
}

pub trait With: Sized {
    fn who(self, name: &str) -> Self;
    fn page(self, n: u32) -> Self;
    fn title_page(self) -> Self;
    fn unreadable(self) -> Self;
    fn styled(self, s: &str) -> Self;
    fn scene_no(self, n: &str) -> Self;
    fn dual_right(self) -> Self;
    fn fmt(self, family: Option<FamilyBucket>, size: Option<SizeStep>) -> Self;
    fn width(self, w: f64, full: bool) -> Self;
}

impl With for ScreenplayElement {
    fn who(mut self, name: &str) -> Self {
        self.character = Some(name.to_owned());
        self
    }
    fn page(mut self, n: u32) -> Self {
        self.page_num = n;
        self
    }
    fn title_page(mut self) -> Self {
        self.is_title_page = true;
        self
    }
    fn unreadable(mut self) -> Self {
        self.is_readable = false;
        self
    }
    fn styled(mut self, s: &str) -> Self {
        self.styled_text = Some(s.to_owned());
        self
    }
    fn scene_no(mut self, n: &str) -> Self {
        self.scene_number = Some(n.to_owned());
        self
    }
    fn dual_right(mut self) -> Self {
        self.dual_right = Some(true);
        self
    }
    fn fmt(mut self, family: Option<FamilyBucket>, size: Option<SizeStep>) -> Self {
        self.fmt = Some(Fmt { family, size });
        self
    }
    fn width(mut self, w: f64, full: bool) -> Self {
        let img = self.img.as_mut().expect("width on an image element");
        img.width_pct = w;
        img.full_page = full;
        self
    }
}
