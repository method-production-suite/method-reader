//! Port of src/epub/css.ts: the reflowable screenplay stylesheet,
//! Kindle-safe, parameterized by FormatOptions
//! (docs/formatting-options-log.md is the knob registry).
//!
//! The rules, from the TypeScript's header (docs/screenplay-format-reference.md
//! has the math, docs/device-map.md §6 what each renderer honors):
//! - HORIZONTAL structure in %: Kindle strips max-width and prescribes
//!   percentage side margins; % scales with screen width.
//! - VERTICAL rhythm in em: scales with the reader's font size; print's one
//!   blank Courier line between elements is 1em.
//! - No line-height/font-size overrides on body text: Enhanced Typesetting
//!   owns within-paragraph spacing.
//! - NO background-color on html or body, ever: the KFX converter then
//!   synthesizes a wrapper block of its own and every keep in the book dies
//!   silently.
//! - CSS value SYNTAX stays CSS-2.1-vintage: no min()/clamp()/var(), which
//!   can blank a whole book on Adobe RMSDK.
//!
//! Break control is deliberately small; every avoid link grows the chunk a
//! renderer pushes. The whole inventory (the guard test pins it):
//!     break-after: avoid   h2.scene-heading (gated: keepSceneHeadingWithScene, on)
//!     break-after: avoid   p.character
//!     break-after: avoid   p.parenthetical (the cue chain, registry #8b)
//!     break-after: avoid   p.mini-slug (secondary sluglines, registry #5b)
//!     break-before: avoid  p.transition
//!     break-inside: avoid  table.dual-dialogue
//!     break-inside: avoid  .dialogue-block (gated: keepSpeechesWhole, off)
//! The cue keep is a CHAIN on paragraphs, not a wrapper (#8b). Outside that
//! list: `page-break-before: always` on section.scene (scenePageBreaks,
//! off), the forced breaks around p.figure-full (only in a book that shows a
//! figure, #19), and widows/orphans on p.action and p.dialogue
//! (printSplitMinimums, on, #17). The multicol shadow rules re-spell the
//! keeps for engines that read only that vocabulary, DERIVED from the keep
//! set so they cannot drift.
//!
//! The figure rules (registry #19) are in the sheet only when the body shows
//! a figure, so a script without pictures gets the sheet it always got, byte
//! for byte. The sheet's own comments ship (and are inlined into the
//! preview), so they are the TypeScript's word for word, including the one
//! that points at src/epub/css.ts for the reasons; the reasons are:
//! 1. The width is a CLASS (p.w5 to p.w100), not an inline style: the reader
//!    preview's CSP refuses `style=`. An explicit width on a replaced element
//!    is its used width, so w100 is exactly the column.
//! 2. No keep on .figure: the box holds one line box, so there is no break
//!    inside it to forbid (measured on Blink, 2026-08-11).
//! 3. p.figure-full's forced breaks sit on the <p>, never a wrapper (#8b),
//!    and the plate has NO vertical margin: a top margin kept after a forced
//!    break pushed a page-tall plate onto the next page (2026-10-01).
//! 4. Height is unbounded; max-height is no remedy, since Kindle strips it.

use sp_model::{CueAlignment, FontFamily, FormatOptions};

use crate::js;

fn font_stack(f: FontFamily) -> &'static str {
    match f {
        FontFamily::Courier => "\"Courier Prime\", \"Courier New\", Courier, monospace",
        FontFamily::Serif => "serif",
        FontFamily::Sans => "sans-serif",
    }
}

/// `` em(v) = `${Number(v.toFixed(2))}em` ``.
fn em(v: f64) -> String {
    format!("{}em", js::number(js::parse_number(&js::to_fixed(v, 2))))
}

/// The stylesheet for one book. `has_figures` says whether the body shows at
/// least one picture (`used_images` non-empty): a fact about this book, not
/// a reader's choice, so it is not a FormatOptions knob.
#[allow(clippy::too_many_lines)]
pub fn screenplay_css(o: &FormatOptions, has_figures: bool) -> String {
    let gap = o.element_spacing_em;
    let scene_break = if o.scene_page_breaks {
        "\nsection.scene { page-break-before: always; }\n"
    } else {
        ""
    };
    // Ragged-right by default; explicit `left` overrides a reader that
    // justifies body text by default.
    let body_align = if o.justify_text { "justify" } else { "left" };
    // Atomic speeches (registry #8c): opt-in.
    let speech_keep = if o.keep_speeches_whole {
        "\n  page-break-inside: avoid;\n  break-inside: avoid;"
    } else {
        ""
    };
    // Every wrapper keep, so the column spelling is DERIVED from it.
    let mut column_keeps = vec!["table.dual-dialogue"];
    if o.keep_speeches_whole {
        column_keeps.push(".dialogue-block");
    }
    let mut column_after = if o.keep_scene_heading_with_scene {
        vec!["h2.scene-heading"]
    } else {
        Vec::new()
    };
    column_after.extend(["p.mini-slug", "p.character", "p.parenthetical"]);
    let column_before = ["p.transition"];
    // The heading keep is a CHAIN, not a wrapper (registry #5a).
    let heading_keep = if o.keep_scene_heading_with_scene {
        "\n  page-break-after: avoid;\n  break-after: avoid;"
    } else {
        ""
    };
    // Print split minimums (registry #17): OFF emits 1, not nothing.
    let min_lines = if o.print_split_minimums { 2 } else { 1 };
    let figures = if has_figures {
        figure_rules(gap)
    } else {
        String::new()
    };
    let cue_rule = match o.cue_alignment {
        CueAlignment::Centered => "  text-align: center;".to_owned(),
        CueAlignment::Indented => format!("  margin-left: {}%;", js::number(o.cue_indent_pct)),
    };
    let paren_rule = match o.cue_alignment {
        CueAlignment::Centered => "  text-align: center;".to_owned(),
        CueAlignment::Indented => format!(
            "  margin-left: {}%;\n  margin-right: {}%;",
            js::number(o.parenthetical_indent_pct),
            js::number(js::round(o.parenthetical_indent_pct / 2.0))
        ),
    };
    let side = js::number(o.dialogue_side_margin_pct);

    let mut s = String::with_capacity(8192);
    s.push_str(
        "html, body {
  margin: 0;
  padding: 0;
}

body {
  font-family: ",
    );
    s.push_str(font_stack(o.font_family));
    s.push_str(";\n}\n");
    s.push_str(scene_break);
    s.push_str(
        "
h2.scene-heading {
  font-size: 1em;
  font-weight: bold;
  text-transform: uppercase;
  margin: ",
    );
    s.push_str(&format!("{} 0 {} 0;", em(gap * 1.6), em(gap)));
    s.push_str(heading_keep);
    s.push_str(
        "
}

span.scene-number {
  font-weight: normal;
}

span.underline {
  text-decoration: underline;
}

/* Original PDF page numbers — a marginal reference, out of the flow so
   they cost no line. Horizontal in %, per the CSS invariants. Dimming is
   opacity rather than a fixed gray, which would fight dark and sepia
   themes; opacity dims against whatever the theme's text color is, and
   engines lacking it just render full strength (registry #13a). */
span.page-marker {
  float: right;
  font-size: 0.75em;
  opacity: 0.6;
  margin-left: 1%;
}

p {
  margin: 0;
  text-indent: 0;
}

p.action {
  margin: ",
    );
    s.push_str(&format!(
        "{} 0;\n  text-align: {body_align};\n  widows: {min_lines};\n  orphans: {min_lines};\n}}\n",
        em(gap)
    ));
    s.push_str(
        "
p.mini-slug {
  font-weight: bold;
  text-transform: uppercase;
  margin: ",
    );
    s.push_str(&format!("{} 0 {} 0;", em(gap * 1.4), em(gap)));
    s.push_str(
        "
  page-break-after: avoid;
  break-after: avoid;
}

/* Print geometry, reflowed: dialogue is a narrow column (58% of the body
   column in print). Cue and parenthetical indent WITHIN the column as
   percentages of it (print: cue +1.2\" and parenthetical +0.6\" into the
   3.5\" column). */
.dialogue-block {
  margin-top: ",
    );
    s.push_str(&format!(
        "{gap};\n  margin-bottom: {gap};\n  margin-left: {side}%;\n  margin-right: {side}%;",
        gap = em(gap)
    ));
    s.push_str(speech_keep);
    s.push_str(
        "
}

/* A cue must never orphan from its dialogue at a page break. Centered
   alignment reads naturally at any screen width; indented reproduces
   print's fixed offsets (which only optically center on paper). */
p.character {
",
    );
    s.push_str(&cue_rule);
    s.push_str(
        "
  page-break-after: avoid;
  break-after: avoid;
}

/* Chained forward as of 2026-09-14, reversing a deliberate refusal. #5 is
   right that every avoid link grows the chunk a renderer pushes, and while
   the cue keep was a WRAPPER that argument held: the chunk already carried
   the cue and the speech, and one more link made a blank-bottom page. With
   the wrapper gone the chunk is cue + parenthetical + the orphans minimum
   (#17), about four lines, which is the same bound #5a accepted when it
   traded the heading wrapper for a chain.

   Without this link a cue and its parenthetical strand together, which is
   the defect #8b exists to prevent wearing a different hat. */
p.parenthetical {
",
    );
    s.push_str(&paren_rule);
    s.push_str(
        "
  page-break-after: avoid;
  break-after: avoid;
}

p.dialogue {
  margin: 0;
  text-align: ",
    );
    s.push_str(&format!(
        "{body_align};\n  widows: {min_lines};\n  orphans: {min_lines};\n}}\n"
    ));
    s.push_str(
        "
/* Simultaneous speech: two half-width columns, kept on one page. The
   table spans full width (wider than the dialogue column) by design. */
table.dual-dialogue {
  width: 100%;
  border-collapse: collapse;
  margin: ",
    );
    s.push_str(&em(gap));
    s.push_str(
        " 0;
  page-break-inside: avoid;
  break-inside: avoid;
}


table.dual-dialogue td {
  width: 50%;
  vertical-align: top;
  padding: 0 2%;
}

/* A transition belongs to the shot before it: it may end a page, never
   begin one (the universal print rule, docs/pagination-reference.md §2 —
   that doc lands from branch worktree-device-map). */
p.transition {
  text-align: right;
  text-transform: uppercase;
  margin: ",
    );
    s.push_str(&em(gap));
    s.push_str(
        " 0;
  page-break-before: avoid;
  break-before: avoid;
}

p.centered {
  text-align: center;
  margin: ",
    );
    s.push_str(&em(gap));
    s.push_str(" 0;\n}\n");
    s.push_str(&figures);
    s.push_str(
        "
/* Title page */
section.titlepage {
  text-align: center;
  margin-top: 20%;
}

section.titlepage h1 {
  font-size: 1.6em;
  text-transform: uppercase;
  font-weight: bold;
  margin-bottom: 2em;
}

section.titlepage p.credit {
  margin: 0.4em 0;
}

section.titlepage p.author {
  font-size: 1.1em;
  margin: 0.4em 0;
}

/* Block font shifts the PDF carried (registry #18), gated by
   preserveFontShifts. Sizes are em, per the vertical-in-em invariant, and
   carry no line-height. A family class sits on the paragraph itself, so it
   locally overrides the body fontFamily option (#6) without !important.
   Values stay CSS-2.1-vintage: RMSDK can blank a whole book on a value
   function it cannot parse. */
.fmt-mono {
  font-family: ",
    );
    s.push_str(font_stack(FontFamily::Courier));
    s.push_str(
        ";
}

.fmt-serif {
  font-family: ",
    );
    s.push_str(font_stack(FontFamily::Serif));
    s.push_str(
        ";
}

.fmt-sans {
  font-family: ",
    );
    s.push_str(font_stack(FontFamily::Sans));
    s.push_str(
        ";
}

.fmt-cursive {
  font-family: \"Comic Sans MS\", cursive;
}

.fmt-minus1 {
  font-size: 0.85em;
}

.fmt-plus1 {
  font-size: 1.2em;
}

.fmt-plus2 {
  font-size: 1.5em;
}

/* ── the multicol spelling of every break rule above ──────────────────

   Some engines paginate by laying the book out in CSS multi-column and
   showing one column per screen, and they read ONLY the multicol
   vocabulary for fragmentation: Apple Books, whose WebKit honors nothing
   else, and the Readium family (Thorium, Kobo's phone and tablet apps).

   SEPARATE rules on purpose, and last on purpose. Separate because iBooks
   drops BOTH spellings when they share one declaration block. Last because
   these selectors repeat ones used above, and a shadow placed first would
   make ruleFor() in the tests answer with the spelling nothing else reads
   (the incident 3abeba3 is named for). Unguarded by @supports on purpose:
   the engines that need this largely predate it.

   Derived from the same gating that emits the real rules, never a second
   list kept by hand. A hand-kept copy once left the whole-speech keep
   silently inert in Apple Books while the cue keep worked. */
",
    );
    s.push_str(&format!(
        "{} {{ -webkit-column-break-inside: avoid; }}\n\n{} {{ -webkit-column-break-after: avoid; }}\n\n{} {{ -webkit-column-break-before: avoid; }}\n",
        column_keeps.join(", "),
        column_after.join(", "),
        column_before.join(", ")
    ));
    s
}

/// The figure rules (registry #19), with their own leading newline so that
/// when absent the seam between p.centered and the title page is the one
/// blank line it always was.
fn figure_rules(gap: f64) -> String {
    let widths: Vec<String> = (1..=20)
        .map(|i| format!("p.w{n} img {{ width: {n}%; }}", n = i * 5))
        .collect();
    format!(
        "
/* A picture carried out of the source PDF (registry #19). These rules are
   only in a book that shows one. No keep, on purpose: this box holds one
   line box, so there is nothing inside it to keep together. The reasons in
   full are in src/epub/css.ts, above screenplayCss; this file is generated
   from it, so that header is not here. */
.figure {{
  margin: {} 0;
  text-align: center;
}}

/* Only the width is authored, so the height follows the aspect ratio. A
   defensive restatement of the initial value, against a reading system
   whose own stylesheet sets a height on images. */
.figure img {{
  height: auto;
}}

/* The width the picture had on the printed page, as a share of the text
   column to the nearest five. A class rather than an inline style, which
   the reader preview refuses. */
{}

/* A plate that filled its printed page gets a page here too: before puts it
   at the top of one, after stops the next paragraph sharing what is left.
   No margin: the plate owns its page, and a top margin kept after the break
   is what pushes a page-tall plate onto the next page and leaves a blank
   one in front of it. */
p.figure-full {{
  margin: 0;
  page-break-before: always;
  page-break-after: always;
}}
",
        em(gap),
        widths.join("\n")
    )
}

/// `SCREENPLAY_CSS`: the default book's sheet.
pub fn default_css() -> String {
    screenplay_css(&FormatOptions::default(), false)
}
