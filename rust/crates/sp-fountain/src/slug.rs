//! Port of `src/fountain/slug.ts`: the stage-2 mini-slug discriminator
//! every token renderer imports (registry #5b).
//!
//! The stage-2 counterpart of the parser's own `PRIMARY_SLUG` in
//! classify.ts: that copy classifies from PDF geometry before the .fountain
//! exists, this one from fountain-js tokens after it. They stay separate
//! because the .fountain text is the boundary between the two stages.

use std::sync::LazyLock;

use sp_model::{Token, TokenType};

use crate::jsre::Regex;

/// The openers fountain-js accepts for an UNFORCED scene heading (its own
/// `rules.scene_heading`, first alternative), as JavaScript source with the
/// `i` flag. A `scene_heading` token whose text fails this could only have
/// come from the forced `.SLUG` form, which is how serialize writes the
/// parser's mini-slugs and how writers write secondary sluglines by hand.
/// classify.ts carries the same literal, and the two are pinned together.
pub const PRIMARY_SLUG_SOURCE: &str = r"^(?:\*{0,3}_?)?(?:(?:int|i)\.?\/(?:ext|e)|int|ext|est)[. ]";

static PRIMARY_SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(PRIMARY_SLUG_SOURCE, "i"));

/// `PRIMARY_SLUG.test(text)`.
pub fn is_primary_slug(text: &str) -> bool {
    PRIMARY_SLUG.test(text)
}

/// `isMiniSlug(t)`: a scene heading that does not open like a real one.
pub fn is_mini_slug(t: &Token) -> bool {
    t.kind == TokenType::SceneHeading && !is_primary_slug(t.text_or_empty())
}
