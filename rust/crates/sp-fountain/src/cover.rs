//! Port of `src/fountain/cover.ts`: the book's cover, chosen once for every
//! renderer, so the EPUB and the MOBI of one script never disagree about it.

use sp_model::{FormatOptions, Token, TokenType};

use crate::notes::{ImageSet, renders_image};

/// The picture a book wears as its cover, and the note that placed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    /// The picture's filename, as its note names it.
    pub file: String,
    /// The note's index in the tokens [`pick_cover`] was given. The
    /// TypeScript hands back the token object and renderers compare by
    /// identity; a renderer that leaves the cover out of its flow leaves out
    /// THIS note, not every note naming the same file.
    pub index: usize,
}

/// `pickCover`: the designed title card, if the script opens with one.
///
/// The rule is about POSITION. Walking the body in order, the cover is a
/// picture note that is the first thing the body would show (notes that draw
/// nothing are passed over, and so is the title page block; ANY other token
/// ahead of it means no cover), renders by [`renders_image`]'s answer, and is
/// full-page (the first picture decides). Off unless `promoteCoverImage` and
/// `includeImages` are both on.
pub fn pick_cover<S: ImageSet + ?Sized>(
    tokens: &[Token],
    format: &FormatOptions,
    have: &S,
) -> Option<Cover> {
    if !format.promote_cover_image || !format.include_images {
        return None;
    }
    for (index, t) in tokens.iter().enumerate() {
        if t.is_title() {
            continue;
        }
        if t.kind != TokenType::Note {
            return None;
        }
        let Some(img) = renders_image(t.text_or_empty(), format.include_images, have) else {
            continue;
        };
        return img.full_page.then_some(Cover {
            file: img.file,
            index,
        });
    }
    None
}
