//! Content for the torture fixture. Data only, no layout logic. Was
//! tools/torture-content.py.
//!
//! This is the file a human edits, so it deliberately carries no geometry:
//! the fixture module owns wrapping, pagination and PDF emission.
//!
//! Every registry behavior this exercises has a row in
//! tools/torture-manifest.json. Edit the two together, or
//! tests/torture-coverage.test.ts will say so.
//!
//! CONFIDENTIALITY: every title, author, character and location here is
//! invented. Nothing from the gitignored root /fixtures/ may ever appear in
//! this file. It is committed, public, and shows up in screenshots.

/// One entry of the content: an element, or a pagination directive.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// (kind, text): text may carry inline markup ({b}...{/b} and kin).
    El(&'static str, String),
    /// Start a new page.
    PageBreak,
    /// Pad with blanks until the page is at line n.
    AtLine(usize),
    /// Two-column simultaneous dialogue: the left and right columns.
    Dual(Vec<(&'static str, String)>, Vec<(&'static str, String)>),
}

fn el(kind: &'static str, text: &str) -> Item {
    Item::El(kind, text.to_string())
}

fn col(rows: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    rows.iter().map(|(k, t)| (*k, (*t).to_string())).collect()
}

pub const TITLE: &[(f64, &str)] = &[
    (4.0, "THE PROOF SHEET"),
    (4.5, "Written by"),
    (5.0, "A. N. Placeholder"),
];

/// Saturation cast, deliberately small so the roster stays realistic.
/// Forty one-off names would stress cue detection but produce a roster no
/// real script has, which would confuse what the saturation pages measure.
pub const CAST: &[&str] = &["BUNNY", "CASSIUS", "ODILE", "WREN"];

pub const NUMBERS: &[&str] = &[
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "twenty-one",
    "twenty-two",
    "twenty-three",
    "twenty-four",
    "twenty-five",
    "twenty-six",
    "twenty-seven",
    "twenty-eight",
    "twenty-nine",
    "thirty",
    "thirty-one",
    "thirty-two",
    "thirty-three",
    "thirty-four",
    "thirty-five",
    "thirty-six",
    "thirty-seven",
    "thirty-eight",
    "thirty-nine",
    "forty",
];

const KEEP: &str = "If this line sits alone at the top or the bottom of a page, the keep failed. ";

/// Numbered speeches for the saturation pages.
///
/// Lengths cycle 1/2/4/3 rather than repeating one shape. A uniform period
/// could align with the device's page height and put every break in the
/// same relative position, saturating nothing. Speeches name their own
/// number so a device defect is reportable as "speech twenty-seven" rather
/// than "somewhere in the back half".
pub fn saturation() -> Vec<Item> {
    let shapes = [1, 2, 4, 3];
    let mut out = Vec::new();
    for (i, n) in NUMBERS.iter().enumerate() {
        let who = CAST[i % CAST.len()];
        let lines = shapes[i % shapes.len()];
        let mut body = format!("Speech {n}. ");
        if lines == 1 {
            body.push_str("Short.");
        } else {
            body.push_str(&KEEP.repeat(lines - 1));
        }
        out.push(el("character", who));
        out.push(el("dialogue", body.trim()));
    }
    out
}

/// The whole of the torture script after its title page.
pub fn content() -> Vec<Item> {
    let mut out = vec![
        // ---- page 1: sluglines of every shape ---------------------------
        el("scene", "INT. ARCHIVE BASEMENT - NIGHT"),
        el(
            "action",
            "Steel shelving to the ceiling. A single bulb, swinging.",
        ),
        el("scene", "EXT. LOADING DOCK - CONTINUOUS"),
        el(
            "action",
            "Rain on corrugated tin, loud enough to argue over.",
        ),
        el("scene", "INT./EXT. DELIVERY VAN - MOVING - LATER"),
        el("action", "The wipers lose."),
        el("scene", "42 INT. ARCHIVE BASEMENT - RESHELVING - DAY 42"),
        el(
            "action",
            "A dual-margin scene number, the shooting-script shape: the \
             same number printed at both margins.",
        ),
        Item::PageBreak,
        // ---- page 2: transitions, mini-slugs, furniture ------------------
        el("scene", "INT. READING ROOM - DAY"),
        el(
            "action",
            "ODILE sets down a box that is heavier than it looks.",
        ),
        // NOT "THE INDEX CARDS": ACTION_PRONOUNS in classify.ts refuses a
        // line opening with "The ", so a mini-slug starting that way is
        // action before the slug rule ever sees it. Real mini-slugs rarely
        // take an article.
        el("mini", "INDEX CARDS"),
        el(
            "action",
            "Handwriting from four decades, none of it consistent.",
        ),
        el("mini", "LEDGER, LATER"),
        el(
            "action",
            "One column, ruled in pencil, never once balanced.",
        ),
        el("trans", "CUT TO:"),
        el("scene", "INT. STAIRWELL - CONTINUOUS"),
        el("action", "Footsteps going down two at a time."),
        el("trans", "DISSOLVE TO:"),
        el("scene", "EXT. ARCHIVE - NIGHT"),
        el("action", "The building keeps its lights on for nobody."),
        el("trans", "SMASH CUT TO:"),
        Item::PageBreak,
        // ---- page 3: cue extensions and hybrids --------------------------
        el("scene", "INT. ARCHIVE BASEMENT - NIGHT"),
        el("action", "Nobody is where their voice says they are."),
        el("character", "BUNNY"),
        el("paren", "(O.S.)"),
        el(
            "dialogue",
            "The period-full spelling, the one the manuals print.",
        ),
        el("character", "CASSIUS"),
        el("paren", "(V.O)"),
        el(
            "dialogue",
            "And the one writers actually type, missing its period.",
        ),
        el("character", "ODILE"),
        el("paren", "(O.C)"),
        el("dialogue", "Off camera, also missing its period."),
        el("character", "WREN (CONT'D)"),
        el("dialogue", "A continued cue, carrying its extension."),
        el("character", "DR. E. T. MARCHETTI"),
        el(
            "dialogue",
            "Dotted abbreviations sitting inside a character name.",
        ),
        el("character", "BUNNY (V.O.)"),
        el("dialogue", "A hybrid: name and extension on one cue line."),
        Item::PageBreak,
        // ---- page 4: parentheticals and roster rescue --------------------
        el("scene", "INT. READING ROOM - DAY"),
        el("action", "The box is open now. It was not worth the trip."),
        el("character", "BUNNY"),
        el("paren", "(quietly)"),
        el(
            "dialogue",
            "A leading parenthetical, before a word is spoken.",
        ),
        el("character", "CASSIUS"),
        el("dialogue", "A line, and then a beat in the middle of it."),
        el("paren", "(reconsidering)"),
        el("dialogue", "And then the rest of it, after the beat."),
        el("character", "ODILE"),
        el("paren", "(standing)"),
        el("paren", "(then, to WREN)"),
        el(
            "dialogue",
            "Two stacked parentheticals before any dialogue at all.",
        ),
        el("character", "WREN"),
        el("dialogue", "Short."),
        Item::PageBreak,
        // ---- page 5: dialogue extremes, and the (MORE) anchor ------------
        el("scene", "INT. ARCHIVE BASEMENT - NIGHT"),
        el("action", "They have been at this for six hours."),
        el("character", "WREN"),
        el("dialogue", "No."),
        el("character", "BUNNY"),
        el("dialogue", "Yes."),
        el("character", "WREN"),
        el("dialogue", "No."),
        // Anchored so the speech below straddles the page break and the
        // parser has to rejoin it across the (MORE) and (CONT'D) furniture a
        // real script prints there. Doubles as the long-dialogue case.
        Item::AtLine(44),
        el("character", "ODILE"),
        el(
            "dialogue",
            "MORE-ANCHOR. This speech begins near the foot of the page on purpose, \
             so that it runs over the break and has to be put back together on the \
             other side. It keeps going well past the boundary so the rejoin has \
             something substantial to work with, and so that the same speech \
             doubles as the long-dialogue case the coverage table asks for, rather \
             than spending two constructions on what is really one shape.",
        ),
        Item::PageBreak,
        // ---- page 6: rich formatting -------------------------------------
        el("scene", "INT. CONSERVATION BENCH - DAY"),
        el("action", "The label reads {b}DO NOT LAMINATE{/b}, twice."),
        el(
            "action",
            "She says it {i}again{/i}, and this time somebody writes it down.",
        ),
        el(
            "action",
            "The stamp is {b}{i}both bold and italic{/i}{/b} at once.",
        ),
        el(
            "action",
            "A styled run that {b}crosses the wrap boundary because it \
             keeps going for long enough to need a second line{/b}, and \
             then stops.",
        ),
        el("action", "Punctuation only{b},{/b} styled by itself."),
        el(
            "action",
            "This word is {u}underlined{/u} with drawn vector art.",
        ),
        // Three drawn rules that are NOT underlines, one per rejection
        // filter. See styled_row_ops in the fixture module for what each
        // one draws.
        el(
            "action",
            "This phrase is {k}struck through{/k}, above the baseline.",
        ),
        el(
            "action",
            "A table rules a border {r}below this cell{/r}, further down.",
        ),
        el(
            "action",
            "A page-wide rule crosses {w}this whole line{/w} of text.",
        ),
        // Block-level font shifts (registry 18). Each is a WHOLE line in the
        // deviant font: fmt is block-level by design, and an inline shift is
        // an explicit non-goal. Keep these SHORT: the layout measures every
        // line at Courier 12, so an 18pt line of 60 characters would run off
        // the page.
        el("action", "{f}CHYRON: EVERY NAME HERE IS INVENTED.{/f}"),
        el("action", "{t}A footnote, set smaller than the body.{/t}"),
        // Mixed case on purpose. "INSERT: THE INDEX CARD" classifies as a
        // MINI-SLUG (INSERT: is a secondary-slug keyword), and a mini-slug
        // never carries a note (markers break slug recognition, registry
        // 5b/9d), so that phrasing would silently test nothing.
        el("action", "{z}The index card, enlarged for the reader.{/z}"),
        el("action", "{g}A SIGN, VERY LARGE INDEED{/g}"),
        el("character", "WREN"),
        el(
            "dialogue",
            "{f}A text message, in the phone's own face.{/f}",
        ),
        el("character", "CASSIUS"),
        el("dialogue", "Dialogue can be {b}bold{/b} too."),
        el("character", "ODILE"),
        el("dialogue", "And {i}italic{/i}, which is the common one."),
        Item::PageBreak,
        // ---- page 7: dual dialogue, short then tall ----------------------
        el("scene", "INT. READING ROOM - DAY"),
        el("action", "They speak over each other."),
        Item::Dual(
            col(&[
                ("character", "BUNNY"),
                ("dialogue", "LEFTMARK. Short, on the left."),
            ]),
            col(&[
                ("character", "CASSIUS"),
                ("dialogue", "RIGHTMARK. Short, on the right."),
            ]),
        ),
        el("action", "And again, at length."),
        Item::Dual(
            col(&[
                ("character", "ODILE"),
                (
                    "dialogue",
                    "This column runs long enough that the taller of the two \
                     passes twelve estimated rendered lines, which is the \
                     threshold where the side-by-side table gives up and both \
                     speeches become ordinary sequential dialogue instead.",
                ),
            ]),
            col(&[
                ("character", "WREN"),
                (
                    "dialogue",
                    "And this one answers at similar length so neither column \
                     is trivially short, because the fallback measures the \
                     taller of the pair and a stub would never reach it.",
                ),
            ]),
        ),
        Item::PageBreak,
    ];
    out.extend(saturation());
    out.extend([
        // ---- the pictures (registry #19 and #19a) ------------------------
        //
        // Three sheets after the saturation run, each built around one
        // picture that the fixture module draws beneath its text
        // (torture_placements). That is also where each picture's geometry,
        // and its margin to the threshold that governs it, is stated. The
        // other two pictures need no text here: the title card sits behind
        // the title sheet, and the letterhead is on every sheet as furniture
        // the book must leave out.
        Item::PageBreak,
        el("scene", "INT. ART DEPARTMENT - DAY"),
        // Rows 2..12 belong to the storyboard panel. The heading takes row 0
        // and its blank row 1, so padding to 14 puts this action a full line
        // clear of the panel's bottom edge rather than grazing it:
        // images.pdf's sheet 2 reserves the same band for the same picture.
        Item::AtLine(14),
        el(
            "action",
            "WREN pins a storyboard panel to the wall, printed small.",
        ),
        el("character", "WREN"),
        el("dialogue", "Under half a column wide, and not a thumbnail."),
        Item::PageBreak,
        // A scene heading with a full-page plate directly under it and
        // nothing else on the sheet. The plate's forced break before it
        // overrides the heading's keep-with-next, so the device pass is asked
        // whether the heading ends up alone at the foot of a page or travels
        // with the plate.
        el("scene", "INT. MAP ROOM - NIGHT"),
        Item::PageBreak,
        // The tall plate: its height in the BOOK follows the source aspect
        // (1:2.75) at the width its note asks for, so on a 3:4 e-reader
        // screen it comes out taller than the page at any font size, and the
        // device pass is asked whether a reader slices it, scales it or clips
        // it. It is under the full-page threshold on purpose: a full-page
        // figure's forced breaks would give it a page of its own and hide the
        // very overflow being asked about.
        el("scene", "INT. FIRE STAIRS - CONTINUOUS"),
        el("action", "A production still, printed tall."),
    ]);
    out
}
