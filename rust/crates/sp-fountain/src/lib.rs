//! Method Reader's Fountain stage, ported from `src/fountain/` and fountain-js.
//!
//! | Module | Ports |
//! |---|---|
//! | [`serialize`] | `serialize.ts`: elements to the `.fountain`, title meta, `disarmBoneyard` |
//! | [`notes`] | `notes.ts`: note stripping, `[[fmt:]]` and `[[img:]]` parsing, the image allowlist |
//! | [`slug`] | `slug.ts`: `PRIMARY_SLUG` and `isMiniSlug` |
//! | [`cover`] | `cover.ts`: which picture is the book's cover |
//! | [`tokenizer`] | fountain-js 1.2.4: `.fountain` to the renderers' tokens |
//! | [`jsre`], [`js`] | JavaScript regex and string semantics the ports run on |
pub mod cover;
pub mod js;
pub mod jsre;
pub mod notes;
pub mod serialize;
pub mod slug;
pub mod tokenizer;

pub use cover::{Cover, pick_cover};
pub use notes::{
    ImageSet, fmt_classes, fmt_size_step, image_note_files, is_safe_image_file, parse_img_note,
    renders_image, strip_notes, strip_screepub_notes,
};
pub use serialize::{TitleMeta, disarm_boneyard, extract_title_meta, to_fountain};
pub use slug::{PRIMARY_SLUG_SOURCE, is_mini_slug, is_primary_slug};
pub use tokenizer::{FountainError, tokenize};
