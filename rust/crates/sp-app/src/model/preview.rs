//! The book's pages as the window draws them.
//!
//! `desktop/ui` drew the engine's own preview document (the EPUB's XHTML and
//! `src/epub/css.ts`) in a browser frame. egui has no HTML engine, so the
//! engine hands the window this instead: the same blocks `html.ts` writes,
//! in the same order, with the formatting the options resolved to. The
//! decisions stay the engine's; only the drawing is the window's. That is a
//! recorded parity gap (rust/PARITY.md R18, the design doc's first item).

/// One run of text with the emphasis the script gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

/// What a block is, by the class `html.ts` gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    SceneHeading,
    Action,
    Centered,
    Transition,
    MiniSlug,
    Character,
    Parenthetical,
    Dialogue,
    /// A picture: PNG bytes and its share of the column, in percent.
    Figure {
        png: Vec<u8>,
        width_pct: u8,
    },
}

/// One block of the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub spans: Vec<Span>,
    /// The printed page that starts here (`.page-marker`), when shown.
    pub page_marker: Option<String>,
    /// The scene number the script carried, when shown.
    pub scene_number: Option<String>,
    /// Which speech this block belongs to (`.dialogue-block`), so a cue, its
    /// parentheticals and its lines are laid out as one; None outside one.
    pub speech: Option<u32>,
    /// For dual dialogue: 0 for the left column, 1 for the right.
    pub column: Option<u8>,
}

impl Block {
    pub fn new(kind: BlockKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            spans: vec![Span::plain(text)],
            page_marker: None,
            scene_number: None,
            speech: None,
            column: None,
        }
    }

    /// The block's words, without its page marker: what `page-show.js`'s
    /// blockWords read and what read.js's headingText kept.
    pub fn words(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// One scene (`section.scene`): its id and its blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub id: String,
    pub blocks: Vec<Block>,
}

impl Scene {
    /// The heading's words, without the page number the engine may carry
    /// inside it (read.js headingText): the scene's first block, when it is
    /// a heading; an empty heading is the book's Opening.
    pub fn heading(&self) -> String {
        match self.blocks.first() {
            Some(b) if b.kind == BlockKind::SceneHeading => b.words(),
            _ => String::new(),
        }
    }
}

/// The typeface the options chose (`fontFamily`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Courier,
    Serif,
    Sans,
}

/// What the options resolved to, as the drawing needs it. Horizontal in
/// percent of the column and vertical in em, the EPUB stylesheet's rule.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewStyle {
    pub face: Face,
    pub element_spacing_em: f32,
    pub dialogue_side_margin_pct: f32,
    /// None when character names are centred.
    pub cue_indent_pct: Option<f32>,
    pub parenthetical_indent_pct: f32,
    pub justify: bool,
    pub dual_side_by_side: bool,
}

impl Default for PreviewStyle {
    fn default() -> Self {
        Self {
            face: Face::Courier,
            element_spacing_em: 1.0,
            dialogue_side_margin_pct: 20.0,
            cue_indent_pct: None,
            parenthetical_indent_pct: 10.0,
            justify: false,
            dual_side_by_side: true,
        }
    }
}

/// The whole preview.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    pub scenes: Vec<Scene>,
    pub style: PreviewStyle,
}

impl Preview {
    /// Whether there is anything to read (read.js readerState's `blank`).
    pub fn is_blank(&self) -> bool {
        self.scenes.iter().all(|s| s.blocks.is_empty())
    }

    /// Every heading and paragraph, in order, with the scene it is in: what
    /// page-show.js reads as the book's blocks (`h2, p`).
    pub fn blocks(&self) -> Vec<&Block> {
        self.scenes.iter().flat_map(|s| s.blocks.iter()).collect()
    }
}
