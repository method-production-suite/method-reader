//! Port of `src/parser/artwork.ts`: what separates a picture from furniture
//! (registry #19).
//!
//! Every threshold here exists because a real PDF is full of images nobody
//! wants in a book: spacer pixels, logo glyphs, revision bullets, letterhead,
//! and watermarks printed behind the script. Carrying those would be worse
//! than carrying nothing, because the reader cannot tell which failure they
//! are looking at.
//!
//! PROVENANCE, and read this before moving anything. Every number below was
//! chosen BY EYE from the August design document on 2026-08-06, against no
//! real script: `tests/fixtures/images.pdf` was built to exercise these
//! rules, so it could only ever show they are self-consistent, never that
//! they are right.
//!
//! AUDITED 2026-08-12 against the private corpus, and NOTHING MOVED. Seven
//! real documents — five feature screenplays, a 110-page
//! script-plus-lookbook, and a 30-page pitch deck — 207 painted placements,
//! 83 kept, 58 refused by these rules, 66 lost to the cap. Every kept
//! picture was looked at. In the two documents that BOTH carry pictures and
//! pass the guards there were no false positives and no false negatives: the
//! lookbook's 22 keeps are its poster, its reference stills and its cast
//! portraits, and its 12 refusals are one ghosted key-art plate repeated
//! behind eleven sheets plus one 84x17 blank strip, with no logo, letterhead
//! or rule anywhere in the kept set. What the audit changed is the
//! confidence, not the constants — each note below now carries the margin
//! the corpus left it, and the two claims the corpus contradicted (the width
//! floor's independence, and "an oversized box is unreachable") are
//! corrected where they were made.
//!
//! They are still NOT equally load-bearing: MIN_HEIGHT_FRAC can barely fire
//! at all, which the corpus confirmed rather than merely predicted, and
//! FURNITURE_SHARE_MIN_PAGES is the one value here that is derived rather
//! than guessed. `dropped_by` on the verdict is what made the audit possible
//! at all: it says which rule killed a picture, which is the entire content
//! of the question "which threshold do I move".
//!
//! NOTHING here may depend on the PDF layer. That absence is the split, not
//! a coincidence: images.rs changes when the PDF reader does, and this file
//! changes when a real script proves a threshold wrong; those two edits are
//! never in the same commit. png.rs draws the same line around itself for
//! the same reason.
//!
//! These thresholds are NOT format options (the August design allowed no
//! size options, and today's spec has "No greyscale or size controls in this
//! design"): they are documented behavior with a rationale, like the
//! revision-star and page-furniture rules. They are also written down twice
//! — `make-fixture.py` states each fixture picture's margin to the ONE
//! threshold that governs it — so tests/artwork.rs reproduces those six
//! boxes and pins both copies of the number at once.

// Every threshold below is compared in the NaN-safe form `!(value >= floor)`
// rather than `value < floor`; see `keep_artwork` for why.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::js;

/// Rendered size floors, as shares of the page.
///
/// Three numbers, and only two of them can fire on a box that FITS on the
/// sheet. The width floor is meant to earn its place on its own: a tall
/// narrow strip — a sidebar rule, a vertical revision bar — clears 3% of the
/// page area while being 8% of its width, and MIN_WIDTH_FRAC would be the
/// only thing that stops it. The height floor cannot do the same, because
/// with the area floor at the same 3% a box under 3% of page height only
/// reaches 3% of page area by being WIDER than the page. It stays for the
/// box that is: a full-bleed banner overhanging the sheet. Note which way
/// round that is — the redundancy is a coincidence of two numbers happening
/// to be equal today, not a property of the rules, and lowering
/// MIN_AREA_FRAC alone would put the height floor back to work.
///
/// 2026-08-12, and it is the first of the two things in this file the
/// corpus CONTRADICTED: neither linear floor was individually load-bearing
/// on it. All 23 width drops and both height drops were also under the area
/// floor, so with either linear check deleted the corpus's verdicts would be
/// unchanged. The tall narrow strip is arithmetic rather than an observation
/// — real, since 8% by 40% is 3.2% of a page, but no document has yet
/// produced one. Keep both floors: their cost is zero (the corpus's nearest
/// kept picture is 13.1% of page width against the 10% floor, and the
/// nearest refused is 9.6%, an empty band either side) and the case they
/// cover is one the area floor genuinely cannot see. Just do not read them
/// as pulling their weight today.
///
/// What 3% of a page IS, since a bare fraction anchors nothing: on US Letter
/// it is 14,541 pt², about 1.9 by 1.45 inches at 4:3. Comfortably under a
/// storyboard panel, comfortably over a revision bullet. What it does NOT
/// clear is branding — the fixture's letterhead logo is 4.1% of its page and
/// sails through this floor, which is why the furniture rule exists. Read
/// this floor as aimed at decoration, never at logos, and do not raise it to
/// try to catch one: at 5% it starts eating small inline panels instead.
///
/// That last sentence was a guess and is now a measurement, which is why
/// this floor is the tightest number in the file. The smallest picture the
/// corpus KEPT THAT A READER WANTED is 3.30% of its page (a cast portrait in
/// the lookbook, one of three on that sheet at 3.30-3.40%), and the largest
/// the AREA rule refused is 2.54%. Real art and real decoration are 0.8 of a
/// percentage point apart here, with the floor between them. 3.5% would
/// already cost three portraits; 5% is not a distant hazard.
///
/// "That a reader wanted" is load-bearing and not a hedge, so read the two
/// populations apart before moving anything. The RAW smallest keep across
/// the corpus is lower: 3.02%, re-measured 2026-08-12, and it is design
/// decoration in the pitch deck rather than art anyone asked for — the same
/// deck whose headings are bitmaps, which no geometric rule can see and
/// which the not-a-screenplay guard refuses before any of it renders. So the
/// floor has 0.8 of a point of daylight against ART and about half a point
/// against EVERYTHING, and the second number is the one a retune actually
/// has to clear. The raw figure and the art margin are two different pairs,
/// so quote them apart.
pub const MIN_AREA_FRAC: f64 = 0.03; // of the page
pub const MIN_WIDTH_FRAC: f64 = 0.10; // of page width
pub const MIN_HEIGHT_FRAC: f64 = 0.03; // of page height

/// Source pixels required on EACH edge. Not a share of anything: the
/// question is whether there is enough picture there to be worth carrying at
/// all, and a 16x16 logo glyph is not, however large the page draws it.
/// Judged on the source size the paint operator declared rather than on what
/// we encoded, though below this floor the two are the same number anyway:
/// downscale_rgb only ever shrinks, and only past its max-edge limit.
///
/// The loosest number here after the 2026-08-12 corpus pass, and
/// deliberately left alone: it fired exactly once in 207 placements, on an
/// 84x17 blank strip, while the smallest edge on any picture the corpus kept
/// was 110 px. Nothing lives in the band between, so there is no evidence
/// for a number and no reason to move one.
pub const MIN_PIXELS: f64 = 32.0;

/// Repetition that makes a picture furniture.
///
/// The sibling of `suppress_boilerplate` in boilerplate.rs, which does this
/// for TEXT: a line recurring on enough distinct pages is a watermark rather
/// than dialogue. Same shape, three deliberate differences. Sameness is a
/// hash of the ENCODED PNG rather than normalized text, because a logo drawn
/// on every page is frequently a distinct XObject per page and identical
/// pixels are the fact we care about. The share is lower (25% against 40%)
/// and combines with the flat page count by OR rather than boilerplate's
/// `max`, so images are caught earlier: a false positive there hides a line
/// of a script, where a false positive here loses a picture.
///
/// FURNITURE_SHARE_MIN_PAGES belongs to the SHARE, not to the flat count,
/// and it is the one number in this file that is derived rather than chosen.
/// The share alone calls a single unrepeated picture furniture in any
/// document of four sheets or fewer — 1 of 4 is 25% — and in a one-page
/// document it is certain of it at 100%. That is a category error rather
/// than a threshold being too tight: "repeated" needs something to repeat
/// against, so the floor is 2 and could not honestly be anything else. With
/// it in place the share's whole remaining job is "in a document of eight
/// sheets or fewer, twice is enough", which is the short-document case it
/// was added for.
///
/// 2026-08-12: the best-evidenced rule in the file, and the one with the
/// clearest blind spot. It accounts for 19 of the corpus's 58 refusals and
/// every one was right — a key-art plate that the lookbook prints ghosted
/// behind eleven of its sheets, and a decorative picture frame on four
/// sheets of the pitch deck.
///
/// THOSE TWO NUMBERS DO ADD UP, and the unit is why it looks otherwise: 11
/// and 4 are SHEETS, 19 is PLACEMENTS, and the frame is painted TWICE on
/// each of its four sheets. 11 + 8 = 19. Re-measured 2026-08-12 on the deck,
/// which is still in the corpus: one hash, four distinct pages, eight
/// candidates. Worth spelling out because `dropped_by` is quoted by page
/// count everywhere else in this file and reports placements, and reading
/// one as the other is how "19" and "11 and 4" came to sit two paragraphs
/// apart looking like a contradiction. Worth knowing about that plate: the
/// XObject is at FULL opacity and the ghosting is a graphics-state alpha
/// applied where it is painted, so a book that kept it would show eleven
/// full-strength copies of the poster, not eleven faint ones. What the page
/// looks like is not what the picture is. The plate is also the case that
/// justifies the rule existing separately from the background one: ten of
/// those eleven sheets carry fewer than twenty text lines (17, 16, 4, 10,
/// 13, 10, 9, 16, 16 and 0 against 28 on the eleventh), so the background
/// rule would have let ten copies through. Nothing legitimate came near: no
/// picture the corpus KEPT appears on more than two sheets.
///
/// THE BLIND SPOT, and it is the hash: a logo on 23 of the pitch deck's 30
/// sheets was a DIFFERENT encoded picture on every sheet — same 278x139
/// source size, same placement, 23 distinct hashes — so this rule never saw
/// it as repeated at all and the size floors caught it instead. Hashing
/// bytes beats hashing the PDF's object id (see ImageCandidate::hash) and
/// both lose to a producer that re-rasterizes its furniture per page.
/// Nothing to fix here without a perceptual comparison, which is a
/// different rule and not a threshold; recorded so the next person does not
/// read the eleven-sheet success as the general case.
pub const FURNITURE_PAGES: usize = 3;
pub const FURNITURE_FRAC: f64 = 0.25;
pub const FURNITURE_SHARE_MIN_PAGES: usize = 2;

/// Covering this much of a page THAT ALSO HOLDS this much text: a
/// background.
///
/// Neither half means anything alone — the text count is the only thing
/// that tells a watermark printed behind the script from a designed title
/// card of the same geometry.
///
/// 20 lines is against the roughly 55 a US Letter screenplay page holds
/// (make-fixture.py's LINES_PER_PAGE), so read it as "more than about a
/// third of a page of text", not as a bare count. That is the number to move
/// if this rule ever misfires, and it moves in a narrow corridor: down near
/// 5 and a title card carrying a credit block reads as a background, up near
/// 40 and a watermark under a half-full page gets carried into the book.
///
/// 2026-08-12, and the corpus's answer is one sample sitting exactly on the
/// line, so read it as support rather than as proof. The rule fired ONCE in
/// 207 placements, on a full-bleed still covering 135% of a pitch-deck sheet
/// that carried exactly 20 lines of text, which is a backdrop and was
/// rightly refused. Fourteen other pictures covering 90% of a sheet or more
/// were KEPT across three documents, on sheets carrying 0 to 12 lines, and
/// they include both of the full-page plates that went on to become covers —
/// so the text half is what did the discriminating, as this note claims,
/// rather than the area half doing all the work. What no document has yet
/// supplied is a sheet of 13 to 19 lines under a full-bleed picture, which
/// is exactly where a wrong number would show. Do not move it on one sample.
pub const BACKGROUND_AREA_FRAC: f64 = 0.90;
pub const BACKGROUND_TEXT_LINES: usize = 20;

/// Art occupying this share of its page is a full-page image. It DESCRIBES
/// a kept image rather than selecting one, so no rule above consults it; it
/// lives beside the other judgements rather than beside the PDF walk for the
/// same reason everything else in this file does. Loose enough to catch a
/// title card inside its printer margins (the fixture's is 75.6%), tight
/// enough that a half-page storyboard strip does not qualify — which is the
/// band, not the exact value, that tests/artwork.rs pins. The corpus
/// (2026-08-12) left the band uncontested rather than confirming the value:
/// its two full-page plates are both over 100% of their sheet and everything
/// else it kept out of a real screenplay is under 19%, so nothing has yet
/// landed between 19% and 100% for this number to rule on.
pub const FULL_PAGE_AREA_FRAC: f64 = 0.70;

/// Did this image cover enough of its sheet to be a full-page one?
///
/// The arithmetic and not just the constant, so the ImageRef builder in
/// lib.rs cannot get the units wrong: both arguments are PERCENTAGES of the
/// page and FULL_PAGE_AREA_FRAC is a FRACTION, so each is divided down
/// before they multiply. Written out once here rather than at the call site
/// because the naive `(w * h) / 100 >= FRAC * 100` happens to be equivalent
/// and reads like a mistake, and the next person to touch it would have to
/// redo the algebra.
///
/// Beside `column_width_pct` for the same reason it is: the two of them are
/// everything the parser decides ABOUT a kept picture.
///
/// The two parameters are named for the reader, not for the arithmetic:
/// area is a product, so swapping them is EXACTLY equivalent (IEEE 754
/// multiplication commutes) and the call site cannot get their order wrong.
/// Said here because a mutation sweep transposing them survives, and that is
/// the mutant being equivalent rather than a test missing — do not add an
/// assertion chasing it.
pub fn is_full_page(page_width_pct: f64, page_height_pct: f64) -> bool {
    (page_width_pct / 100.0) * (page_height_pct / 100.0) >= FULL_PAGE_AREA_FRAC
}

/// The share of an 8.5" sheet a standard script's TEXT COLUMN occupies.
///
/// 1.5" of left margin and 1.0" of right leaves 6.0", which is 70.6% of the
/// page. Not measured from any PDF: it is the format's own margin spec, the
/// same one docs/screenplay-format-reference.md states, and a script that
/// departs from it is departing from the format rather than from a
/// threshold this file guessed.
const TEXT_COLUMN_FRAC: f64 = 0.706;

/// The floor `column_width_pct` clamps to, and it is NOT reachable through
/// `keep_artwork`: MIN_WIDTH_FRAC already refuses anything under 10% of the
/// page, and 10 / 0.706 is 14. It exists for a caller that has not been
/// through the filters — and because a floor is where the NaN guard below
/// has somewhere to land. Read the 5 itself in sp-model
/// (`ImageRef::width_pct`), which states both ends of the range for the
/// renderers.
const MIN_COLUMN_PCT: f64 = 5.0;

/// Rendered width as a share of the script's TEXT COLUMN, not of the page.
///
/// The e-book has no print margins to speak of, so the column is the honest
/// reference: an image drawn as wide as the dialogue around it should render
/// as wide as the dialogue around it. Measuring against the page instead
/// would set every picture about 30% too narrow.
///
/// Here rather than in images.rs because remapping a page fraction to a
/// column fraction is a judgement about how a script is SET — the same kind
/// of thing as every threshold above — and not a fact the PDF reported. It
/// is the reason this file is used by lib.rs as well as by images.rs.
///
/// The upper clamp is ordinary rather than defensive: anything wider than
/// the column (a full-bleed title card is ~121% of it) is asked to fill the
/// column and no more. The lower one is the unreachable case described
/// above.
pub fn column_width_pct(page_width_pct: f64) -> f64 {
    let of_column = page_width_pct / TEXT_COLUMN_FRAC;
    // `!(x > floor)` and not `x < floor`, the house form in this file: NaN
    // fails every comparison, so the natural clamp returns NaN and the width
    // in the `[[img: ...]]` note becomes `NaN`, which is an invalid CSS
    // length far from whatever produced it. Unreachable through
    // `keep_artwork` — its own safe-form area floor already refused any box
    // whose arithmetic was not numbers — and written this way so that stays
    // a property of THIS function too.
    if !(of_column > MIN_COLUMN_PCT) {
        return MIN_COLUMN_PCT;
    }
    js::round(of_column.min(100.0))
}

/// Ceiling on one document, because nothing else in the pipeline bounds the
/// size of the book.
///
/// The arithmetic, so a maintainer facing a 90-picture lookbook can see what
/// they are trading, against the 50 MB limit on the Send-to-Kindle EMAIL
/// route (docs/device-map.md, T5 — the tightest of the delivery routes; the
/// web upload allows 200 MB).
///
/// 2026-08-07, MEASURED, and it is worse than the August design's estimate:
/// sixty pictures out of a real 30-page pitch deck came to 16.3 MB, not the
/// ~6 MB a 100 KB panel predicts. A panel averages nearer 270 KB once it is
/// a full-bleed plate rather than a storyboard thumbnail, which is exactly
/// what a lookbook is made of. So the headroom against the email route is
/// about 3x, not the order of magnitude the estimate suggested, and the
/// binding constraint arrives near 180 pictures rather than 500.
///
/// Sixty still stands, and the measurement is the argument FOR it rather
/// than against: this is the conservative end of a range whose upper bound
/// is much closer than anyone thought. Raise it only with a measurement, not
/// with the estimate. The reasons not to are conversion time and the
/// picture folder, plus now the email route actually being in sight.
///
/// How often it bites, 2026-08-12: one document in seven, and that one
/// offered 126 pictures that had cleared every filter, so 66 were dropped.
/// The only real SCREENPLAY carrying art in the corpus offered 22. The cap
/// is an exception in the shape of document this feature is for, which is
/// why its warning can afford to be loud.
pub const MAX_IMAGES: usize = 60;

/// `ImageCandidate`: one placement, as the filters judge it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageCandidate {
    pub page_num: u32,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// SOURCE pixels as the paint operator declared them, never the encoded
    /// PNG's: this is the picture's own resolution, which is what MIN_PIXELS
    /// asks about. The two names sit one letter apart downstream, so the
    /// distinction is spelled out here as well as in images.rs.
    pub src_px_width: f64,
    pub src_px_height: f64,
    /// hash of the ENCODED bytes: the same logo is often a different objId
    /// per page, and identical pixels are the fact we actually care about
    pub hash: String,
    /// Text lines on this candidate's PAGE, for the background rule. Named
    /// for the page and not for the image because that is what it is: a
    /// page property carried per candidate, so every candidate on a sheet
    /// repeats one number. Redundant rather than wrong — the collector fills
    /// a page's candidates in a single loop from a single count, so they
    /// cannot disagree, and the alternative is a second parameter and a
    /// lookup for a rule that fires on roughly one image per corpus.
    pub page_text_lines: usize,
}

/// Which rule refused a candidate. Ordered as the filter applies them, so
/// the first one that fires is the one reported: a 20x20pt image at 16x16
/// pixels is `pixels`, not `area`, because that is the cheaper question and
/// it is asked first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DropRule {
    Pixels,
    Width,
    Height,
    Area,
    Furniture,
    Background,
}

/// `Record<DropRule, number>`, in the TypeScript's key order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedBy {
    pub pixels: u32,
    pub width: u32,
    pub height: u32,
    pub area: u32,
    pub furniture: u32,
    pub background: u32,
}

impl DroppedBy {
    fn count(&mut self, rule: DropRule) {
        let slot = match rule {
            DropRule::Pixels => &mut self.pixels,
            DropRule::Width => &mut self.width,
            DropRule::Height => &mut self.height,
            DropRule::Area => &mut self.area,
            DropRule::Furniture => &mut self.furniture,
            DropRule::Background => &mut self.background,
        };
        *slot += 1;
    }

    /// The count for one rule.
    pub fn get(&self, rule: DropRule) -> u32 {
        match rule {
            DropRule::Pixels => self.pixels,
            DropRule::Width => self.width,
            DropRule::Height => self.height,
            DropRule::Area => self.area,
            DropRule::Furniture => self.furniture,
            DropRule::Background => self.background,
        }
    }
}

/// What survived, how many the CAP refused, and what killed the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtworkVerdict<T> {
    pub kept: Vec<T>,
    /// Images that cleared every filter and lost to MAX_IMAGES alone.
    ///
    /// Deliberately NOT a count of everything dropped — that is
    /// `dropped_by`. The filters dropping letterhead is the normal case,
    /// nearly every real script has some, and a caller warning about it
    /// would warn on every conversion, which is how a warning stops being
    /// read. The cap is the exceptional one, and the August design asked for
    /// a warning, as today's spec does ("keeping the first 60 and
    /// warning"), precisely because silent truncation reads as "we carried
    /// everything" when we did not. `kept.len() + over_cap` is what a caller
    /// needs to say "this script has 73 pictures and Method Reader carried 60".
    pub over_cap: usize,
    /// How many candidates each rule refused, keyed by the rule that fired
    /// first.
    ///
    /// This is the retune instrument, and it exists because a corpus diff
    /// can only ever show that a picture VANISHED. "Which threshold do I
    /// move" is the whole content of a retune, and without attribution
    /// answering it means bisecting the constants by hand — or building a
    /// document that isolates one rule, which is a luxury only a fixture
    /// has. A histogram over a real corpus says directly whether the pixel
    /// floor is doing the work or the furniture rule is.
    ///
    /// Counts CANDIDATES, which means PLACEMENTS and not sheets and not
    /// distinct pictures: a logo on forty sheets is forty, and a frame
    /// painted twice on each of four sheets is eight. That is the right unit
    /// for "how much of this document did each rule eat", and it means the
    /// six counts plus `kept.len()` plus `over_cap` add up to the number of
    /// candidates handed in.
    ///
    /// Say which unit whenever one of these numbers is written down. Every
    /// note about the furniture rule is phrased in SHEETS, because that is
    /// what the rule tests; this is in placements; and quoting the two side
    /// by side is how a correct pair of measurements came to read as a
    /// contradiction (see FURNITURE_PAGES above).
    pub dropped_by: DroppedBy,
}

/// Apply every filter, and take the first MAX_IMAGES of what is left.
///
/// `page_count` is the DOCUMENT's, not the run's: the furniture share has to
/// be measured against the whole script, or a five-page excerpt reads its
/// own header as art.
///
/// `candidates` is expected in document order, and the cap trusts that: it
/// keeps a PREFIX. Preferring larger images instead would be wrong twice
/// over — the cap exists to bound the size of the book, so keeping the
/// biggest is backwards on the cap's own rationale, and a size-ranked subset
/// illustrates page 3 and page 40 and nothing between, which reads to a
/// reader as damage rather than as a limit. A prefix degrades the way a
/// reader can understand: the art stops.
///
/// ONE page geometry for a whole document, and that is an ASSUMPTION rather
/// than a fact. The collector reads a viewport per page but hands this
/// function a single width and height — the document's MOST COMMON, as
/// extract.rs is wired (`dominant_page_size`). On a uniform script, which is
/// nearly all of them, that is exact. On a production draft carrying a
/// LANDSCAPE storyboard insert — precisely the document this feature exists
/// for — that insert is measured against a portrait page: its `w_frac` goes
/// above 1.0, its area fraction inflates past what it really covers, and the
/// background rule can then fire on a legitimate full-bleed landscape
/// picture sitting on a text-heavy sheet. Taking the majority geometry
/// rather than any one page's bounds that error to the odd sheet instead of
/// spreading it across the document, and is as far as a single width and
/// height can go; the real fix is page dimensions on `ImageCandidate` rather
/// than parameters here, which is a change to the collector as much as to
/// this function. Named and not fixed.
pub fn keep_artwork<T: Borrow<ImageCandidate> + Clone>(
    candidates: &[T],
    page_width: f64,
    page_height: f64,
    page_count: u32,
) -> ArtworkVerdict<T> {
    // Not reachable through extract.rs, which takes both from a page's own
    // viewport at scale 1. One line so that stays true for the next caller:
    // every rule below divides by these, and at zero the fractions come out
    // Infinity, which clears every FLOOR — the one way a candidate can pass a
    // size test by being unmeasurable rather than by being big. Attributed to
    // no rule, because no rule ran: this is a refusal to judge, not a
    // judgement.
    if !(page_width > 0.0) || !(page_height > 0.0) {
        return ArtworkVerdict {
            kept: Vec::new(),
            over_cap: 0,
            dropped_by: DroppedBy::default(),
        };
    }

    // Distinct pages per identical picture, tallied over EVERY candidate
    // rather than only the ones that clear the size floors. Whether something
    // is furniture is a question about the picture and the document, not
    // about one placement's size: letterhead scaled down on the title page is
    // still letterhead. The August design's wording ("does not appear on 3 or
    // more pages") had no size qualifier in it, and neither does today's
    // spec's ("the same picture on 3 or more pages").
    let mut pages_by_hash: HashMap<&str, HashSet<u32>> = HashMap::new();
    for c in candidates {
        let c: &ImageCandidate = c.borrow();
        pages_by_hash
            .entry(c.hash.as_str())
            .or_default()
            .insert(c.page_num);
    }

    // The rule that refuses this candidate, or None to keep it.
    //
    // Flat early returns rather than a predicate per rule, deliberately: half
    // the comments in here are about ORDER — which check is cheap, which one
    // must come before which — and pulling `furniture()` and `background()`
    // out into named predicates would strand every one of them at a call site
    // with nothing to attach to.
    let reject = |c: &ImageCandidate| -> Option<DropRule> {
        // Every threshold here is written `!(value >= floor)` rather than
        // `value < floor`, and the difference is NaN: `NaN < 0.10` is false,
        // so the natural form KEEPS a candidate whose numbers are not
        // numbers, and the width that reaches the .fountain note is then
        // `NaN`. These are arithmetic over a CTM this module does not own.
        //
        // Where the safe form actually bites is not symmetric, and it is
        // worth being exact because registry #19 gets written from this file.
        // The PIXEL floor is alone: nothing else reads src_px, so naive there
        // means a NaN reaches the encoder's dimensions unopposed. The three
        // BOX floors share one backstop — `area_frac` is `w_frac * h_frac`,
        // so a NaN in either linear fraction poisons the area too, and the
        // safe AREA check by itself catches every NaN box. So the linear
        // floors are safe-form for symmetry and for surviving a reorder, not
        // because either is individually load-bearing today. All of them, or
        // that reasoning has to be redone by whoever next moves a line.
        // (Mutation-checked both directions: all three naive keeps a NaN box,
        // any one of the three left safe refuses it.)
        //
        // What none of this covers: an OVERSIZED box clears every floor
        // rather than failing it, and nothing here has an upper bound to
        // catch it, because a legitimate picture may overhang the sheet and
        // the bound is therefore not a one-liner.
        //
        // Reachable on real files, not only on a degenerate CTM: the
        // 2026-08-12 corpus pass found a pitch deck painting a real still
        // inside a box 33.6x the page's width, so its area fraction came out
        // at 3357% of the sheet. Nothing bad followed, and that is worth being
        // precise about rather than lucky about — the picture was art and
        // keeping it was right, and `column_width_pct`'s upper clamp meant
        // the book set it at the text column like any other. So the missing
        // bound costs a WRONG-BY-ANY-MEASURE box its refusal, and costs a
        // correct one nothing, which is the measured reason there is no upper
        // bound.
        if !(c.src_px_width >= MIN_PIXELS) || !(c.src_px_height >= MIN_PIXELS) {
            return Some(DropRule::Pixels);
        }

        let w_frac = (c.x1 - c.x0) / page_width;
        let h_frac = (c.y1 - c.y0) / page_height;
        // As the product of the two linear fractions, not (w * h) / pageArea.
        // The same quantity, and it keeps all three floors comparable against
        // the fractions they are stated in.
        let area_frac = w_frac * h_frac;
        // Both linear floors come before the area one, so an inverted box
        // cannot multiply two negative edges back into a positive area and
        // clear it.
        if !(w_frac >= MIN_WIDTH_FRAC) {
            return Some(DropRule::Width);
        }
        if !(h_frac >= MIN_HEIGHT_FRAC) {
            return Some(DropRule::Height);
        }
        if !(area_frac >= MIN_AREA_FRAC) {
            return Some(DropRule::Area);
        }

        let pages = pages_by_hash.get(c.hash.as_str()).map_or(1, HashSet::len);
        if pages >= FURNITURE_PAGES {
            return Some(DropRule::Furniture);
        }
        // Multiplied out rather than `pages / page_count >= FURNITURE_FRAC`,
        // which needs a `max(page_count, 1)` to look safe and does not need
        // it to BE safe: `pages / 0` is Infinity, and Infinity clears the
        // share exactly as dividing by one would. A guard that protects
        // nothing reads as if it does.
        #[allow(clippy::cast_precision_loss)]
        if pages >= FURNITURE_SHARE_MIN_PAGES
            && pages as f64 >= f64::from(page_count) * FURNITURE_FRAC
        {
            return Some(DropRule::Furniture);
        }

        if area_frac >= BACKGROUND_AREA_FRAC && c.page_text_lines >= BACKGROUND_TEXT_LINES {
            return Some(DropRule::Background);
        }

        None
    };

    let mut dropped_by = DroppedBy::default();
    let survivors: Vec<T> = candidates
        .iter()
        .filter(|c| match reject((*c).borrow()) {
            Some(rule) => {
                dropped_by.count(rule);
                false
            }
            None => true,
        })
        .cloned()
        .collect();

    let over_cap = survivors.len().saturating_sub(MAX_IMAGES);
    let kept: Vec<T> = survivors.into_iter().take(MAX_IMAGES).collect();
    ArtworkVerdict {
        kept,
        over_cap,
        dropped_by,
    }
}
