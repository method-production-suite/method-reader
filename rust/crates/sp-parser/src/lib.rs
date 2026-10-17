//! Method Reader's screenplay parser, ported from `src/parser/*`.
//!
//! Two stages, each proved against the TypeScript engine's own answers
//! (`rust/oracle/<fixture>/`):
//!
//! | Stage | Entry | Oracle |
//! |---|---|---|
//! | the PDF's facts → lines and pictures | [`extract::extract_document`] | `pages.json` → `lines.json` |
//! | lines and pictures → elements | [`parse_lines`] | `lines.json` → `elements.json` |
//!
//! | Module | Ports |
//! |---|---|
//! | [`facts`] | the input: what pdf.js told extract.ts (the oracle's `pages.json`) |
//! | [`extract`] | extract.ts: lines, styles, underlines, phantom spaces, dual dialogue, fonts |
//! | [`images`] | images.ts: placements, pixels, PNGs, the document's pile |
//! | [`artwork`] | artwork.ts: the six picture rules |
//! | [`png`] | png.ts: the PNG encoder and the box downscaler |
//! | [`group`] | group.ts: lines into blocks |
//! | [`classify`] | classify.ts: blocks into elements |
//! | [`cue`] | cue.ts: the one definition of a cue's text |
//! | [`rescue`] | rescue.ts: cues the roster vouches for |
//! | [`boilerplate`] | boilerplate.ts: page furniture |
//! | [`title_page`] | title-page.ts |
//! | [`first_page`] | first-page.ts: the window's first-page preview |
//! | [`js`] | the JavaScript string and number semantics the TypeScript relied on |
//!
//! The parser stays FORMAT-OPTION-FREE: nothing here reads FormatOptions.

pub mod artwork;
pub mod boilerplate;
pub mod classify;
pub mod cue;
pub mod extract;
pub mod facts;
pub mod first_page;
pub mod group;
pub mod images;
pub mod js;
pub mod png;
pub mod rescue;
pub mod title_page;

use sp_model::{
    CharacterInfo, ElementType, ImageRef, ParsedScreenplay, RawImage, RawLine, SceneInfo,
    ScreenplayElement,
};

use crate::artwork::{column_width_pct, is_full_page};
use crate::boilerplate::suppress_boilerplate;
use crate::classify::{attach_scene_numbers, classify_block};
use crate::group::group_blocks;
use crate::rescue::rescue_cues;
use crate::title_page::detect_title_pages;

/// Classify already-extracted lines into a structured screenplay — the pure
/// post-extraction pipeline, split out so callers that need the raw lines
/// (e.g. scanned-PDF detection) don't read the PDF twice.
///
/// `images` may be empty, and that is not a degraded mode: a script with no
/// pictures and a caller that does not want them produce the same stream
/// they always did.
///
/// When they ARE passed, each must carry the page and top-edge `y` that
/// `extract_document` gave it — those are what decide where a picture lands
/// in the stream, and a picture placed by `page_num` alone would sit at the
/// top of its sheet. Order does not matter: `insert_images` sorts this half
/// itself. What it CANNOT normalize is `lines`, which must arrive in
/// document order, as `extract_document` returns them.
pub fn parse_lines(lines: &[RawLine], images: &[RawImage]) -> ParsedScreenplay {
    // Element IDs are scoped to this call — not a process-wide counter — so
    // concurrent calls never interleave or reset each other's ID sequence.
    let mut element_counter = 0u64;
    let mut next_id = move || {
        let id = format!("elem{element_counter}");
        element_counter += 1;
        id
    };

    // Step 2: Group into text blocks
    let blocks = group_blocks(lines);

    // Step 3: Classify each block (stateless — passes prev element)
    let mut elements: Vec<ScreenplayElement> = Vec::with_capacity(blocks.len());
    let mut prev: Option<usize> = None;
    for block in &blocks {
        let element = classify_block(block, prev.map(|i| &elements[i]), &mut next_id);
        // Page furniture is TRANSPARENT to classification context. A cue can
        // be the last line of a page with its speech resuming after the
        // printed page number, and letting that number reset the speaker
        // splits the speech: the continuation classifies as action and the
        // cue is left with nothing under it. Device-confirmed on a Kindle
        // (proof sheet, speech 37).
        //
        // This is not "attach whatever follows furniture to the last
        // speaker": classify_block still decides by indent band, so action
        // after a page number stays action. It only stops furniture from
        // erasing the context that a genuine continuation line depends on.
        let furniture = element.kind == ElementType::PageNumber;
        elements.push(element);
        if !furniture {
            prev = Some(elements.len() - 1);
        }
    }

    // Step 4: Attach scene numbers to scene headings (shooting scripts)
    attach_scene_numbers(&mut elements);

    // Step 5: Detect title pages and front matter
    let with_title_pages = detect_title_pages(&elements);

    // Step 5.5: Suppress recurring page furniture (watermarks) — re-types to
    // page-number; must run before metadata so counts stay consistent.
    let page_count_for_suppression = with_title_pages
        .iter()
        .map(|el| el.page_num)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut suppressed = suppress_boilerplate(&with_title_pages, page_count_for_suppression);

    // Step 5.6: Rescue cues the text heuristics rejected. Placement is
    // deliberate. AFTER suppress_boilerplate, because suppression only
    // re-types elements that are still `action` — a recurring watermark that
    // happened to match a character name would otherwise be rescued to
    // `character` first and become permanently immune to suppression. BEFORE
    // extract_characters, so rescued speeches reach the dialogue counts. It
    // also needs `is_title_page`, which only exists after step 5.
    // Steps 4/5/5.5 are all length-preserving, so blocks[i] still pairs with
    // suppressed[i].
    rescue_cues(&mut suppressed, &blocks);

    // Step 5.7: a speech split by a page boundary is ONE speech.
    //
    // When a speech runs over a page in the source PDF, its halves arrive as
    // two dialogue elements with printed page furniture between them. Giving
    // them the same speaker (see the page-number transparency above) is not
    // enough: they still serialize as two fountain lines and render as two
    // paragraphs, so the book shows a hard break mid-sentence.
    // Device-confirmed on a Kindle.
    //
    // Registry 8's (MORE)/(CONT'D) rejoin only fires when the script PRINTS
    // that furniture. Final Draft and Highland do; celtx does not, and shows
    // 9 of these seams. This is the case that rejoin cannot see.
    //
    // Must run AFTER rescue_cues: steps 4 through 5.6 are length-preserving
    // so blocks[i] still pairs with suppressed[i], and this pass is the first
    // that changes the element count.
    let merged = merge_split_speeches(suppressed);

    // Step 5.8: merge the pictures in where they sat on the page (#19).
    //
    // AFTER merge_split_speeches, and both halves of that are load-bearing.
    // Not earlier, because that pass joins a page-split speech only when
    // nothing but furniture sits between its halves, and a plate at the top
    // of the second page, under the page number and above the continuation,
    // sits exactly there: merged in first, it ends the run, and one speech
    // comes out as two with the picture between them. Earlier still would
    // also break `rescue_cues`, which needs `blocks[i]` to pair with
    // `elements[i]`, and an image element has no block. Not later, because
    // `extract_scenes` records an `element_index` into whatever array it is
    // handed, so the images have to be in the stream before the metadata
    // passes count positions in it.
    let with_images = insert_images(merged, images, &mut next_id);

    // Step 6: Build metadata
    let characters = extract_characters(&with_images);
    let scenes = extract_scenes(&with_images);
    let page_count = with_images
        .iter()
        .map(|el| el.page_num)
        .max()
        .unwrap_or(1)
        .max(1);

    ParsedScreenplay {
        elements: with_images,
        characters,
        scenes,
        page_count,
    }
}

/// Join the halves of a speech that a page boundary split in two.
///
/// Two dialogue elements merge only when they name the SAME character and
/// nothing but page furniture sits between them. Keying on the character
/// matters: a page break between two people's lines would otherwise fuse
/// their speeches into one. Anything real in between (action, a heading, a
/// new cue) ends the run, so this can only rejoin what one page turn
/// separated.
///
/// The furniture elements are kept, not dropped: `showPageMarkers`
/// (registry 13a) re-surfaces them, and this pass has no business deciding
/// that.
fn merge_split_speeches(elements: Vec<ScreenplayElement>) -> Vec<ScreenplayElement> {
    let mut out: Vec<ScreenplayElement> = Vec::with_capacity(elements.len());
    // The last dialogue element pushed (its index in `out`), and whether only
    // furniture has been seen since. Reset by anything that is neither.
    let mut open_speech: Option<usize> = None;

    for el in elements {
        if el.kind == ElementType::PageNumber {
            out.push(el);
            continue;
        }

        if let Some(open) = open_speech {
            let speech = &out[open];
            if el.kind == ElementType::Dialogue
                && el.character.is_some()
                && el.character == speech.character
                // ONLY across a page boundary. A speech may legitimately hold
                // two paragraphs separated by a blank line, and those arrive
                // as two dialogue elements on the SAME page. Merging those
                // would destroy a break the writer put there. The defect this
                // fixes is specifically a page turn cutting one paragraph in
                // half.
                && el.page_num != speech.page_num
            {
                let speech = &mut out[open];
                // A source line break inside one speech is a wrap, not a
                // paragraph: join with a space so the sentence reads
                // continuously.
                speech.text = format!("{} {}", speech.text, el.text);
                // The styled fallback reads the text AS JUST JOINED, exactly
                // as the TypeScript's `openSpeech.styledText ?? openSpeech.text`
                // did one line after reassigning it.
                if speech.styled_text.is_some() || el.styled_text.is_some() {
                    let head = speech
                        .styled_text
                        .clone()
                        .unwrap_or_else(|| speech.text.clone());
                    let tail = el.styled_text.as_deref().unwrap_or(&el.text);
                    speech.styled_text = Some(format!("{head} {tail}"));
                }
                continue;
            }
        }

        let is_dialogue = el.kind == ElementType::Dialogue;
        out.push(el);
        open_speech = is_dialogue.then(|| out.len() - 1);
    }

    out
}

/// Merge images into the element stream by where they sat on the page.
///
/// A one-pass MERGE and not a sort of the joined result: sorting THAT would
/// need a stable comparator over two different shapes and would reorder
/// equal-y elements for nothing. The merge instead wants each input ordered
/// by (page ascending, y descending) — PDF user space puts the top of a
/// sheet at the high y — and the two halves get there by different means.
///
/// The IMAGES half is normalized here. `to_raw_images` already sorts on
/// exactly this key, so today the sort is a no-op; it is here because
/// `parse_lines` is public and a caller that builds `RawImage`s some other
/// way would otherwise misplace pictures silently.
///
/// The ELEMENTS half cannot be normalized the same way: re-sorting
/// classified elements would fight `merge_split_speeches` and the
/// dual-dialogue de-interleave, both of which place elements deliberately.
/// So this is the half carrying the assumption, and what it rests on:
///
///   - `group_items_into_lines` emits a page's lines in descending y, and
///     `extract_document` visits pages 1..N and concatenates. `group_blocks`
///     breaks on a page change and `build_block` takes the FIRST line's y,
///     so blocks inherit the order.
///   - Steps 4 through 5.6 are length-preserving and none of them reorders;
///     `merge_split_speeches` only REMOVES elements, and a subsequence of an
///     ordered stream is ordered.
///
/// The ONE exception: the dual-dialogue de-interleave emits a simultaneous
/// exchange as the left column followed by the right column, and the right
/// column's y values restart at the top of the region. An image whose y
/// falls inside that region therefore lands among the LEFT column's lines
/// instead of after the whole exchange. What that costs is a misplaced
/// picture and nothing more (the serializer's lookahead skips images, so
/// both speeches survive), and a picture printed INSIDE a two-column
/// simultaneous speech is a document the corpus does not contain.
fn insert_images(
    elements: Vec<ScreenplayElement>,
    images: &[RawImage],
    next_id: &mut dyn FnMut() -> String,
) -> Vec<ScreenplayElement> {
    if images.is_empty() {
        return elements;
    }
    let mut ordered: Vec<&RawImage> = images.iter().collect();
    js::sort_by(&mut ordered, |a, b| {
        a.page_num
            .cmp(&b.page_num)
            .then_with(|| js::cmp_num(b.y, a.y))
    });

    // The name comes WITH the image: `to_raw_images` in images.rs assigns
    // it, and is the only place that numbers anything.
    let mut as_element = |img: &RawImage| ScreenplayElement {
        kind: ElementType::Image,
        // The filename doubles as the element's text, so the element is
        // never empty: the serializer skips anything that trims to nothing,
        // and an image carries no words of its own. The note itself is
        // written from `img.file`, so this copy is load-bearing only as "not
        // empty".
        text: img.file.clone(),
        // Minted after every text element's, because the block loop has
        // already run. IDs are unique and nothing in the engine reads them in
        // order.
        id: next_id(),
        page_num: img.page_num,
        y: Some(img.y),
        // Never title-page furniture, and this is a decision rather than a
        // shortcut: the serializer joins the readable title-page elements
        // into the book's TITLE, so a flagged image would put "p001-01.png"
        // in the metadata.
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
        img: Some(ImageRef {
            file: img.file.clone(),
            width_pct: column_width_pct(img.page_width_pct),
            full_page: is_full_page(img.page_width_pct, img.page_height_pct),
        }),
    };

    // Does this image belong before this element?
    //
    // STRICTLY greater, and the tie is not arbitrary: the two `y` are
    // different EDGES. A line's is the text item's own origin, which for text
    // is its BASELINE, while an image's is its TOP. So a picture whose y
    // equals a line's y begins where that line's glyphs sit and runs downward
    // from there — it is below the line, and the line goes first.
    //
    // `+infinity` for an element with no `y`, which the parser cannot
    // produce: such an element is skipped over and the image lands against
    // the next element that can be located.
    let goes_before = |img: &RawImage, el: &ScreenplayElement| {
        img.page_num < el.page_num
            || (img.page_num == el.page_num && img.y > el.y.unwrap_or(f64::INFINITY))
    };

    let mut out = Vec::with_capacity(elements.len() + ordered.len());
    let mut i = 0;
    for el in elements {
        while i < ordered.len() && goes_before(ordered[i], &el) {
            out.push(as_element(ordered[i]));
            i += 1;
        }
        out.push(el);
    }
    // Whatever is left sat below the last element, or on a sheet past it — a
    // final full-page plate is exactly that, and it must not be dropped.
    while i < ordered.len() {
        out.push(as_element(ordered[i]));
        i += 1;
    }
    out
}

fn extract_characters(elements: &[ScreenplayElement]) -> Vec<CharacterInfo> {
    // Insertion-ordered: (name, dialogue count, first appearance).
    let mut chars: Vec<(String, u32, u32)> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for (i, el) in elements.iter().enumerate() {
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        // A centered title lands at ~38% indent — inside the cue band — and
        // classifies as a character. detect_title_pages already flagged it;
        // the roster must honor the flag or the title becomes a "speaker".
        if el.is_title_page {
            continue;
        }
        if el.kind == ElementType::Character
            && let Some(base) = el.base_character.as_deref().filter(|b| !b.is_empty())
            && !index.contains_key(base)
        {
            index.insert(base.to_owned(), chars.len());
            chars.push((base.to_owned(), 0, i));
        }
        if el.kind == ElementType::Dialogue
            && let Some(base) = el.character.as_deref().filter(|c| !c.is_empty())
        {
            match index.get(base) {
                Some(&k) => chars[k].1 += 1,
                None => {
                    index.insert(base.to_owned(), chars.len());
                    chars.push((base.to_owned(), 1, i));
                }
            }
        }
    }

    let mut out: Vec<CharacterInfo> = chars
        .into_iter()
        .map(|(name, dialogue_count, first_appearance)| CharacterInfo {
            base_name: name.clone(),
            name,
            dialogue_count,
            first_appearance,
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.dialogue_count));
    out
}

fn extract_scenes(elements: &[ScreenplayElement]) -> Vec<SceneInfo> {
    let mut scene_number = 0;
    elements
        .iter()
        .enumerate()
        .filter(|(_, el)| el.kind == ElementType::Scene)
        .map(|(idx, el)| {
            scene_number += 1;
            SceneInfo {
                heading: el.text.clone(),
                scene_number,
                element_index: u32::try_from(idx).unwrap_or(u32::MAX),
                page_num: el.page_num,
            }
        })
        .collect()
}
