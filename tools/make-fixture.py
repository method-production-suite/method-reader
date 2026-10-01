#!/usr/bin/env python3
"""Generate the public-safe test fixtures in tests/fixtures/.

    make-fixture.py screenplay tests/fixtures/screenplay.pdf
    make-fixture.py prose      tests/fixtures/prose.pdf
    make-fixture.py blank      tests/fixtures/blank-pages.pdf
    make-fixture.py torture    tests/fixtures/torture.pdf
    make-fixture.py demo       tests/fixtures/field-station.pdf
    make-fixture.py images     tests/fixtures/images.pdf
    make-fixture.py jpx        tests/fixtures/jpx.pdf

Real scripts are confidential, so the committed fixtures are invented. The
first three kinds cover the three paths convert.ts can take: a screenplay
that converts, a document that trips the not-a-screenplay guard, and pages
with no text layer that trip the scanned guard. torture exercises every
content-driven registry behavior (tools/torture-content.py), and demo is
Field Station, the invented feature the README and site pictures show
(tools/field-station-content.py). `images` carries pictures, one for each
rule that decides whether a picture is art or furniture, and `jpx` carries a
single JPEG2000 one, the format pdf.js can only decode with a decoder loaded
from outside its bundle.

Geometry from docs/screenplay-format-reference.md — 12pt Courier, 10 chars
per inch, 6 lines per inch. Indents matter: Screepub classifies elements by
horizontal position, so a demo PDF with sloppy geometry would parse wrong
and prove nothing.
"""
import re
import textwrap
import zlib

PT = 72.0
PAGE_W, PAGE_H = 8.5 * PT, 11 * PT
LINE = 12.0                    # 6 lines/inch
TOP = PAGE_H - 1.0 * PT        # 1" top margin
BOTTOM = 1.0 * PT
LINES_PER_PAGE = 55

# Base-14 Type1 fonts, so nothing is embedded and the generator stays a
# pure string builder. Verified against pdf.js: it reports these PostScript
# names through commonObjs, which is what src/parser/extract.ts regexes for
# /bold|black|heavy/ and /italic|oblique/.
FONTS = {"F1": "Courier", "F2": "Courier-Bold",
         "F3": "Courier-Oblique", "F4": "Courier-BoldOblique",
         "F5": "Helvetica"}

CHAR_W = 7.2                   # 12pt Courier advance: 0.1" at 10 chars/inch

X = {                          # inches from left edge
    "scene":  1.5, "action": 1.5, "dialogue": 2.5,
    # Transitions are RIGHT-flush in a real script, and the classifier keys on
    # that: TRANSITION_MIN is indent 55, so a left-flush "CUT TO:" is action,
    # not a transition. At 1.5" neither fixture had ever produced a single
    # transition element while appearing to contain four.
    "paren":  3.1, "character": 3.7, "trans": 6.0, "pgnum": 7.4,
    "mini":   1.5,
}
WRAP = {"scene": 60, "action": 60, "dialogue": 35, "paren": 24,
        "trans": 60, "character": 38,   # cues never wrap in practice
        "mini": 60}

# (type, text) — blank string means a blank line
S = [
    ("scene", "INT. THE LAST VIDEO STORE - NIGHT"),
    ("action", "Fluorescent light hums over aisles of VHS tapes. Rain "
               "streaks the front window. A hand-lettered sign reads: "
               "EVERYTHING MUST GO."),
    ("action", "MARGO SHAW, 60s, cardigan with a pen through the pocket, "
               "alphabetizes a shelf that nobody will browse."),
    ("action", "The bell over the door RINGS."),
    ("character", "MARGO"),
    ("dialogue", "We close at ten."),
    ("character", "DEV"),
    ("paren", "(O.S.)"),
    ("dialogue", "It's nine forty."),
    ("action", "DEV OKONKWO, 20s, soaked through, shakes off a jacket that "
               "was never waterproof."),
    ("character", "MARGO"),
    ("dialogue", "And yet."),
    ("character", "DEV"),
    ("dialogue", "I need something for a fourteen-year-old who thinks "
                 "movies started with streaming."),
    ("action", "Margo looks up for the first time."),
    ("character", "MARGO"),
    ("paren", "(a small conversion)"),
    ("dialogue", "Sit down."),
    ("trans", "CUT TO:"),
    ("scene", "INT. THE LAST VIDEO STORE - HORROR AISLE - CONTINUOUS"),
    ("action", "Margo moves like she's done this ten thousand times, "
               "because she has. Dev trails her, dripping."),
    ("character", "MARGO"),
    ("dialogue", "Fourteen. Scared, or pretending not to be?"),
    ("character", "DEV"),
    ("dialogue", "Pretending. Loudly."),
    ("character", "MARGO"),
    ("dialogue", "Then not the one with the mask. Everyone starts with the "
                 "mask and nobody remembers it."),
    ("action", "She pulls a battered clamshell case and holds it like "
               "evidence."),
    ("character", "MARGO (CONT'D)"),
    ("dialogue", "This one. Nothing happens for forty minutes and then you "
                 "never sleep facing a closet again."),
    ("character", "DEV"),
    ("dialogue", "That's a pitch?"),
    ("character", "MARGO"),
    ("dialogue", "That's a promise."),
    ("trans", "DISSOLVE TO:"),
    ("scene", "EXT. STRIP MALL - NIGHT"),
    ("action", "Rain. The store's neon sign flickers between LAST and AST. "
               "Behind the glass, two silhouettes argue happily about "
               "something that doesn't matter."),
    ("character", "DEV"),
    ("paren", "(O.S.)"),
    ("dialogue", "How long have you been here?"),
    ("character", "MARGO"),
    ("paren", "(O.S.)"),
    ("dialogue", "Since people still rewound."),
    ("scene", "INT. THE LAST VIDEO STORE - COUNTER - LATER"),
    ("action", "A shoebox of index cards. Margo writes on one in careful "
               "block letters and slides it across."),
    ("character", "DEV"),
    ("dialogue", "What's this?"),
    ("character", "MARGO"),
    ("dialogue", "Your card. Nineteen years running, four hundred and six "
                 "members. You're four hundred and seven."),
    ("character", "DEV"),
    ("dialogue", "You're closing in a week."),
    ("character", "MARGO"),
    ("dialogue", "Then it'll be a short membership."),
    ("action", "Dev takes the card anyway. Pockets it like it matters, "
               "because it does."),
    ("character", "DEV"),
    ("dialogue", "What happens to all of it?"),
    ("action", "Margo looks down the aisles. Twenty thousand tapes. A "
               "lifetime of other people's Friday nights."),
    ("character", "MARGO"),
    ("dialogue", "Somebody buys the shelves. The rest goes where the rest "
                 "goes."),
    ("character", "DEV"),
    ("dialogue", "That's grim."),
    ("character", "MARGO"),
    ("paren", "(not grim at all)"),
    ("dialogue", "It's Tuesday."),
    ("trans", "SMASH CUT TO:"),
    ("scene", "INT. DEV'S APARTMENT - NIGHT"),
    ("action", "A television old enough to have a dial. Dev threads the "
               "tape in. Static, then a countdown leader — 5, 4, 3."),
    ("action", "His NIECE, 14, arms folded, sits as far from the screen as "
               "the room allows."),
    ("character", "NIECE"),
    ("dialogue", "This is going to be so boring."),
    ("character", "DEV"),
    ("dialogue", "Forty minutes. Give it forty minutes."),
    ("action", "The countdown hits 1. The room goes dark blue."),
    ("action", "Forty minutes later, she has not moved, and she is sitting "
               "considerably closer."),
    ("trans", "FADE OUT."),
]

TITLE = [
    (4.0, "THE LAST VIDEO STORE"),
    (4.5, "Written by"),
    (5.0, "A. N. Placeholder"),
]


ASCII_MAP = str.maketrans({"—": "--", "–": "-", "’": "'",
                           "‘": "'", "“": '"', "”": '"', "…": "..."})


def esc(s):
    # Courier screenplays use -- and straight quotes by convention, and
    # latin-1 (the PDF string encoding here) has no em dash anyway.
    s = s.translate(ASCII_MAP)
    return s.replace("\\", r"\\").replace("(", r"\(").replace(")", r"\)")


# --- inline markup, for the torture kind ---------------------------------

# b/i are font STYLES; u/k/r/w are DRAWN rules (one real, three decoys);
# f/t/z/g are block font SHIFTS, which registry 18 reads back out of the PDF.
MARKUP = re.compile(r"\{(/?)([biukrwftzg])\}")


def parse_markup(s):
    """'a {b}bold{/b} c' -> ('a bold c', [(2, 6, 'b')]).

    Offsets index the PLAIN text, so wrapping can slice by character and
    re-derive styles without ever seeing a marker. Unbalanced markers raise:
    a typo buried in 14 sheets of content must fail at generation rather
    than produce a fixture that quietly tests nothing.
    """
    plain, spans, open_at, pos, out_len = [], [], {}, 0, 0
    for m in MARKUP.finditer(s):
        plain.append(s[pos:m.start()])
        out_len += m.start() - pos
        closing, style = m.group(1), m.group(2)
        if closing:
            if style not in open_at:
                raise ValueError(f"unmatched {{/{style}}} in {s!r}")
            spans.append((open_at.pop(style), out_len, style))
        else:
            if style in open_at:
                raise ValueError(f"nested {{{style}}} in {s!r}")
            open_at[style] = out_len
        pos = m.end()
    plain.append(s[pos:])
    if open_at:
        raise ValueError(f"unclosed {sorted(open_at)} in {s!r}")
    return "".join(plain), spans


def _words(plain):
    """[(start, end)] for each space-delimited word, as offsets into plain."""
    out, i, n = [], 0, len(plain)
    while i < n:
        while i < n and plain[i] == " ":
            i += 1
        if i >= n:
            break
        j = i
        while j < n and plain[j] != " ":
            j += 1
        out.append((i, j))
        i = j
    return out


def _runs(plain, spans, start, end):
    """Slice [start, end) into styled runs, merging equal-styled neighbours."""
    runs = []
    for i in range(start, end):
        st = sorted(s for (a, b, s) in spans if a <= i < b)
        if runs and runs[-1][0] == st:
            runs[-1][1] += plain[i]
        else:
            runs.append([st, plain[i]])
    return [{"styles": st, "text": txt} for st, txt in runs]


def wrap_spans(plain, spans, width):
    """Greedy word wrap that keeps style spans intact across line breaks.

    -> list of lines, each a list of {styles, text} runs.

    Deliberately NOT textwrap. textwrap takes a plain string, so markup has
    to be either stripped (losing the styles) or left in (counting marker
    characters as text width, and splitting a marker across a line). The
    three original fixture kinds keep calling textwrap, untouched, which is
    what lets the byte-stability guard stay meaningful.
    """
    words = _words(plain)
    if not words:
        return [[{"styles": [], "text": ""}]]
    lines = []
    line_start, line_end = words[0]
    for ws, we in words[1:]:
        if we - line_start <= width:
            line_end = we
        else:
            lines.append(_runs(plain, spans, line_start, line_end))
            line_start, line_end = ws, we
    lines.append(_runs(plain, spans, line_start, line_end))
    return lines


def layout():
    """-> list of pages; each page is a list of (x_inches, text)."""
    flowed = []
    for kind, text in S:
        width = WRAP[kind]
        for i, chunk in enumerate(textwrap.wrap(text, width) or [""]):
            flowed.append((kind, chunk))
        flowed.append(("blank", ""))

    pages, cur, n = [], [], 0
    for kind, text in flowed:
        if kind == "blank":
            if cur:
                cur.append(None)
                n += 1
        else:
            cur.append((X[kind], text))
            n += 1
        if n >= LINES_PER_PAGE:
            pages.append(cur)
            cur, n = [], 0
    if cur:
        pages.append(cur)
    return pages


# Styles that are DRAWN rather than selected. They never change which font a
# run uses, which mirrors how PDFs actually carry underline and is why the
# parser cannot see it from font data alone (registry 9d).
DRAWN = {"u", "k", "r", "w"}

# Block font shifts (registry 18). f picks the second FACE; t/z/g pick a
# SIZE, chosen to land one on each side of every threshold in that entry:
#   10/12 = 0.83 -> -1     15/12 = 1.25 -> +1     18/12 = 1.50 -> +2
SHIFT_SIZE = {"t": 10.0, "z": 15.0, "g": 18.0}
SHIFTS = {"f"} | set(SHIFT_SIZE)


def size_for(styles):
    for s in styles:
        if s in SHIFT_SIZE:
            return SHIFT_SIZE[s]
    return 12.0


def font_for(styles):
    """Style set -> font resource key."""
    if "f" in styles:
        return "F5"                  # a family shift, not a weight or slope
    s = set(styles) - DRAWN - SHIFTS
    if s == {"b", "i"}:
        return "F4"
    if s == {"b"}:
        return "F2"
    if s == {"i"}:
        return "F3"
    return "F1"


def styled_row_ops(x_in, y, runs):
    """-> (text operators, drawn rectangles) for one laid-out line.

    Four rule shapes, one real and three decoys. Each decoy is rejected by a
    DIFFERENT filter in collectUnderlineMarks/markUnderlinesItem, so a
    loosened threshold shows up as a stray underscore in the emitted Fountain:

      u  real underline   0.6pt tall, run-width, 2.0pt under the baseline
      k  strikethrough    same bar at baseline + 3.5 (mid x-height) -> the
                          y-window rejects anything above the baseline
      r  table border     same bar at baseline - 6.0 -> below the window, and
                          6pt ABOVE the next row's baseline, so it is out of
                          that row's window too
      w  page-wide rule   0.5in margins (540pt = 88% of the page) at the real
                          underline offset -> only the furniture-width filter
                          saves it, over real text with 100% overlap
    """
    ops, rects = [], []
    x = x_in * PT
    for run in runs:
        text, styles = run["text"], run["styles"]
        if text:
            ops += [f"/{font_for(styles)} {size_for(styles):g} Tf",
                    f"1 0 0 1 {x:.2f} {y:.2f} Tm", f"({esc(text)}) Tj"]
            w = len(text) * CHAR_W
            # A filled rectangle, not a stroked line. Real generators stroke;
            # filling here proves the detector accepts both paint ops.
            if "u" in styles:
                rects.append(f"0 g {x:.2f} {y - 2.0:.2f} {w:.2f} 0.6 re f")
            if "k" in styles:
                rects.append(f"0 g {x:.2f} {y + 3.5:.2f} {w:.2f} 0.6 re f")
            if "r" in styles:
                rects.append(f"0 g {x:.2f} {y - 6.0:.2f} {w:.2f} 0.6 re f")
            if "w" in styles:
                rects.append(f"0 g {0.5 * PT:.2f} {y - 2.0:.2f} "
                             f"{PAGE_W - 1.0 * PT:.2f} 0.6 re f")
        x += len(text) * CHAR_W
    return ops, rects


# Dual dialogue column geometry. The parser anchors a dual region on a line
# holding TWO cue-shaped clusters, then partitions body lines by start-x
# (registry 10a: the boundary starts at rightCueX - 13% and refines only
# leftward). These indents put the right cue far enough right that the
# learned boundary lands cleanly between the columns.
DUAL_X = {
    "left":  {"character": 2.2, "dialogue": 1.9, "paren": 2.4},
    "right": {"character": 5.4, "dialogue": 5.1, "paren": 5.6},
}
DUAL_WRAP = 26


def dual_rows(left, right):
    """Two (kind, text) lists -> rows where both columns share a Y.

    The cue lines of both columns MUST land on one Y, or the parser sees
    two ordinary consecutive cues rather than a dual region.
    """
    def col(items, side):
        out = []
        for kind, text in items:
            plain, spans = parse_markup(text)
            for runs in wrap_spans(plain, spans, DUAL_WRAP):
                out.append((DUAL_X[side][kind], runs))
        return out

    L, R = col(left, "left"), col(right, "right")
    rows = []
    for i in range(max(len(L), len(R))):
        pair = []
        if i < len(L):
            pair.append(L[i])
        if i < len(R):
            pair.append(R[i])
        rows.append(("multi", pair))
    rows.append(None)
    return rows, len(rows)


def flow_torture(content):
    """Lay torture content out into pages, honouring directives.

      ("pagebreak", "")   start a new page
      ("atline", n)       pad with blanks until the page is at line n
      ("dual", L, R)      two-column simultaneous dialogue
    """
    pages, cur, n = [], [], 0

    def flush():
        nonlocal cur, n
        if cur:
            pages.append(cur)
        cur, n = [], 0

    for kind, *rest in content:
        if kind == "pagebreak":
            flush()
            continue

        if kind == "atline":
            target = rest[0]
            if n > target:
                # Raising rather than overflowing is deliberate. Silently
                # sliding past the anchor would turn the (MORE) test into a
                # test of nothing the first time content above it grew by a
                # sentence, and nothing would report it.
                raise SystemExit(
                    f"atline: page {len(pages) + 1} is already at line {n}, "
                    f"cannot pad back to {target}. Content above it grew: "
                    f"shorten it, or move the anchor.")
            while n < target:
                cur.append(None)
                n += 1
            continue

        if kind == "dual":
            rows, used = dual_rows(rest[0], rest[1])
            if n + used > LINES_PER_PAGE:
                flush()
            cur.extend(rows)
            n += used
            continue

        plain, spans = parse_markup(rest[0])
        for runs in wrap_spans(plain, spans, WRAP[kind]):
            if n >= LINES_PER_PAGE:
                flush()
            cur.append((X[kind], runs))
            n += 1
        # A cue and a parenthetical sit DIRECTLY above what they introduce:
        # real scripts print no blank line there, and the geometry is the
        # whole point of this fixture. Everything else gets its blank.
        # (The inherited layout() blanks after every element; the parser
        # tolerates it, but it is not what a real page looks like.)
        if kind not in ("character", "paren") and n < LINES_PER_PAGE:
            cur.append(None)
            n += 1

    flush()
    return pages


def torture_content_stream(rows, page_no):
    """Like content_stream, but rows carry styled runs and may hold two
    columns sharing one Y."""
    text_ops, rects = ["BT", "/F1 12 Tf"], []
    if page_no:
        text_ops += [f"1 0 0 1 {X['pgnum']*PT:.2f} {PAGE_H - 0.5*PT:.2f} Tm",
                     f"({page_no}.) Tj"]
    y = TOP
    for row in rows:
        if row is not None:
            if row[0] == "multi":
                # KNOWN GAP: these two columns are emitted at correct
                # x positions on a shared Y, but the parser still joins them
                # into "BUNNY CASSIUS" rather than anchoring a dual region.
                # clusterSplit's thresholds all pass by hand-calculation, so
                # the cause is upstream of it and not yet identified.
                # tests/torture.test.ts records the current behavior.
                for x, runs in row[1]:
                    ops, rs = styled_row_ops(x, y, runs)
                    text_ops += ops
                    rects += rs
            else:
                x, runs = row
                ops, rs = styled_row_ops(x, y, runs)
                text_ops += ops
                rects += rs
        y -= LINE
    text_ops.append("ET")
    return "\n".join(rects + text_ops)


def content_stream(rows, page_no):
    out = ["BT", "/F1 12 Tf"]
    if page_no:                                   # page number, top right
        out += [f"1 0 0 1 {X['pgnum']*PT:.2f} {PAGE_H - 0.5*PT:.2f} Tm",
                f"({page_no}.) Tj"]
    y = TOP
    for row in rows:
        if row is not None:
            x, text = row
            out += [f"1 0 0 1 {x*PT:.2f} {y:.2f} Tm", f"({esc(text)}) Tj"]
        y -= LINE
    out.append("ET")
    return "\n".join(out)


def title_stream(table=None):
    out = ["BT", "/F1 12 Tf"]
    for inches_down, text in (TITLE if table is None else table):
        x = (8.5 - len(text) / 10.0) / 2.0        # 10 chars/inch, centered
        y = PAGE_H - inches_down * PT
        out += [f"1 0 0 1 {x*PT:.2f} {y:.2f} Tm", f"({esc(text)}) Tj"]
    out += [f"1 0 0 1 {1.5*PT:.2f} {1.5*PT:.2f} Tm",
            "(Placeholder Pictures) Tj",
            f"1 0 0 1 {1.5*PT:.2f} {1.5*PT - LINE:.2f} Tm",
            "(hello@example.com) Tj", "ET"]
    return "\n".join(out)


# A document that is emphatically not a screenplay: one left margin, no
# sluglines, nothing sitting in the cue/dialogue indent bands. Parses to zero
# scenes and zero dialogue, which is what NotAScreenplayError keys on. The
# ALL-CAPS headings are deliberate — at the body margin they must read as
# action, not as character cues.
PROSE = [
    "THE LAST VIDEO STORE",
    "",
    "A proposal for a documentary feature, prepared for financiers who "
    "have never rewound anything.",
    "",
    "THE OPPORTUNITY",
    "",
    "There were once nine thousand video rental stores in this country. "
    "There are now fewer than a dozen. Each one that closes takes with it "
    "a set of recommendations that existed nowhere else, held by a person "
    "who never wrote any of it down.",
    "",
    "We propose to film the last year of one of them.",
    "",
    "AUDIENCE",
    "",
    "The film is for the generation that grew up inside these rooms and "
    "for the one that has only ever been handed an algorithm. It argues, "
    "gently, that the difference between the two is a person.",
    "",
    "BUDGET",
    "",
    "A crew of four, ninety shooting days, and the patience to wait for a "
    "Tuesday when nothing happens. The full schedule follows overleaf, "
    "along with the letters of access we have already secured.",
    "",
    "This page exists to be rejected by the screenplay guard, and its "
    "prose is padded to make sure the extractor sees a healthy text "
    "layer while the parser finds nothing screenplay-shaped in it.",
]


def prose_stream():
    out = ["BT", "/F1 12 Tf"]
    y = TOP
    for para in PROSE:
        for chunk in textwrap.wrap(para, 72) or [""]:
            if chunk:
                out += [f"1 0 0 1 {1.0*PT:.2f} {y:.2f} Tm", f"({esc(chunk)}) Tj"]
            y -= LINE
    out.append("ET")
    return "\n".join(out)


def blank_stream():
    # A grey box and no text operators at all — the shape of a page scanned
    # to an image. extractDocument finds no lines, so the scanned guard fires.
    return (f"0.85 g {1.0*PT:.2f} {1.0*PT:.2f} "
            f"{PAGE_W - 2*PT:.2f} {PAGE_H - 2*PT:.2f} re f")


# --- the images kind -----------------------------------------------------
# Six pictures, one per rule in the six-rules bullet of
# docs/superpowers/specs/2026-10-01-images-in-the-book-design.md, section
# "What the August branch already settled". Two are art and survive; four are
# furniture of one sort or another and must be dropped. Everything a rule
# keys on is geometry, so the numbers below are the fixture: change one and
# you change which rule fires.
#
# The thresholds, from the same spec (and registry #19), against a 612x792
# page: area 3%, width 10%, height 3%, 32 pixels on either edge; furniture at
# 3 pages, or at 25% of them once a picture is on at least two; background at
# 90% of a page carrying 20+ text lines; full-page at 70%. Every row below
# states its own margin to the ONE threshold that governs it, because a
# retune bites hardest on the KEPT images and a kept image quietly becoming
# dropped fails nothing here: the generator would still emit a perfectly
# good PDF.
#
#   imCard   sheet 1  522x702pt, 261x351px
#            KEPT, full page, cover candidate. 75.6% of the page: 5.6 points
#            over the 70% that makes it full-page, and 14 under the 90% the
#            background rule needs -- on a sheet of 5 text lines, 15 under
#            that rule's 20. Clears every size floor by an order of
#            magnitude.
#   imPanel  sheet 2  180x135pt, 240x180px
#            KEPT, inline. 5.0% of the page against the 3% area floor, 29%
#            wide against 10%, 17% high against 3%. The plan's original
#            144x108pt cleared area by 0.21 points, which is a knife edge
#            for a KEPT image; tests/artwork.test.ts owns the boundary case
#            with hand-built candidates, this one wants headroom.
#   imLogo   all 5 sheets  200x100pt, 100x50px
#            DROPPED: page furniture, on 5 sheets against a 3-sheet
#            threshold and 100% of them against 25%. It must clear every
#            SIZE floor so furniture is the only thing that can drop it,
#            and area is where it is closest: 4.1% against 3%.
#   imTiny   sheet 3  130x130pt, 16x16px
#            DROPPED: pixel floor, 16 against 32. Rendered big on purpose so
#            it clears all three size floors -- 3.5% area against 3%, 21%
#            width against 10%, 16% height against 3%. That area margin is
#            0.5 points: shrink this box and the area floor starts catching
#            it instead, and a loosened pixel floor sails through unnoticed.
#            The thinnest margin in this table, so it is the first to check
#            if the area floor is ever retuned.
#   imSmall  sheet 3  20x20pt, 200x200px
#            DROPPED: rendered-size floor, and it fails all three at once --
#            0.08% area against 3%, 3.3% width against 10%, 2.5% height
#            against 3%. 200x200px so the pixel floor is demonstrably not
#            what caught it.
#   imMark   sheet 4  full bleed, 120x160px
#            DROPPED: background. 100% of a page against 90%, printed
#            beneath 29 text lines against the rule's 20. Clears the pixel
#            floor at 120x160 so that is not what caught it, and sits on one
#            sheet of five (20%, under the 25% furniture share) so that is
#            not either.
#
# One shared fragility that USED to be worth knowing, recorded because the
# arithmetic still looks alarming: imCard, imPanel and imMark are each on a
# single sheet of five, so each is at 20% against the 25% furniture share, and
# on four sheets each would be at exactly 25%. The share alone would make all
# three furniture at once. It does not, because the share needs a picture on
# at least two sheets before it can fire at all (artwork.ts,
# FURNITURE_SHARE_MIN_PAGES) -- repetition needs something to repeat against, and
# without that floor a lone picture in a four-page short would be read as
# letterhead. So the sheet count is free to change here, down to two: imLogo
# is on EVERY sheet, so it is caught by the 3-sheet rule at five and by the
# share at two, and only a one-sheet fixture would leave it as art.

IMAGES = {                     # name -> (px width, px height, gradient seed)
    # The seeds only have to DIFFER. Two images of identical dimensions and
    # identical pixels would encode to identical PNGs, hash alike, and be
    # read as one repeated picture -- which is to say, as furniture.
    "imCard":  (261, 351, 17),
    "imPanel": (240, 180, 53),
    "imLogo":  (100, 50, 101),
    "imTiny":  (16, 16, 149),
    "imSmall": (200, 200, 197),
    "imMark":  (120, 160, 233),
}

# The same corner of every sheet, in the bottom margin where no line of the
# script reaches: a letterhead logo that the furniture rule must catch on
# repetition alone, not on being small or badly placed.
LOGO_W, LOGO_H = 200, 100
LOGO = (PAGE_W - LOGO_W - 0.5 * PT, 0.5 * PT, LOGO_W, LOGO_H)

# (name, x, y, w, h) per sheet, drawn in this order and therefore UNDER the
# text, which is where a watermark and a title card both live in a real PDF.
SHEET_PLACEMENTS = [
    [("imCard", 45, 45, 522, 702), ("imLogo", *LOGO)],
    [("imPanel", 108, 561, 180, 135), ("imLogo", *LOGO)],
    [("imTiny", 108, 400, 130, 130), ("imSmall", 300, 440, 20, 20),
     ("imLogo", *LOGO)],
    [("imMark", 0, 0, PAGE_W, PAGE_H), ("imLogo", *LOGO)],
    [("imLogo", *LOGO)],
]

IMAGE_TITLE = [
    (4.0, "THE PAPER LANTERN"),
    (4.5, "Written by"),
    (5.0, "J. Q. Nobody"),
]

# Sheets 2-5. ("gap", n) reserves n blank lines for a picture: the text and
# the image have to be positioned in one coordinate system, or the sheet
# looks right to a rule and wrong to a reader.
IMAGE_SHEETS = [
    [  # sheet 2: the panel sits in the gap, under the scene heading
        ("scene", "INT. ARCHIVE ROOM - DAY"),
        # The panel spans y 561..696, so its bottom edge falls between the
        # baselines of rows 13 and 14. Reserving 13 rows puts the action at
        # row 15, clearing the picture by a full line rather than grazing it.
        ("gap", 13),
        ("action", "WREN, 30s, holds a storyboard panel up to the window "
                   "and squints at it."),
        ("character", "WREN"),
        ("dialogue", "Somebody drew this in 1974 and nobody has looked at "
                     "it since."),
        ("action", "HOLT sets down a box of film cans."),
        ("character", "HOLT"),
        ("dialogue", "That is the job."),
    ],
    [  # sheet 3: text stops well above the two undersized pictures
        ("scene", "INT. ARCHIVE ROOM - PROJECTION BOOTH - LATER"),
        ("action", "A splice reel turns. The room smells of vinegar and "
                   "dust."),
        ("character", "HOLT"),
        ("dialogue", "Two frames are missing from the head of the roll."),
        ("character", "WREN"),
        ("dialogue", "Then we cut around them."),
        ("action", "She threads the leader by feel, the way she was "
                   "taught."),
    ],
    [  # sheet 4: a full page of text over the watermark. The line COUNT is
       # the point: the background rule needs 20 or more, and this sheet
       # lands at 29 as the parser groups them (28 rows plus the page
       # number). Cut TEN lines here and the watermark becomes art: at 20 it
       # is still background, because the rule fires at 20 or more.
        ("scene", "EXT. LOADING DOCK - NIGHT"),
        ("action", "Rain on corrugated steel. WREN wheels a cart of film "
                   "cans out under a sodium lamp and counts them twice."),
        ("action", "HOLT holds an umbrella over the cart and not over "
                   "himself."),
        ("character", "WREN"),
        ("dialogue", "Forty-one. There were forty-three when we started."),
        ("character", "HOLT"),
        ("dialogue", "Two of them went out for repair in March and nobody "
                     "wrote it down."),
        ("character", "WREN"),
        ("dialogue", "Nobody ever writes it down."),
        ("action", "She pulls a tarp over the cart and ties it at the "
                   "corners."),
        ("character", "HOLT"),
        ("dialogue", "The truck comes at six. Everything on that manifest "
                     "has to be on the dock by then, in order, and dry."),
        ("character", "WREN"),
        ("dialogue", "Then we work through it."),
        ("action", "The lamp buzzes and cuts out. Neither of them stops "
                   "moving."),
        ("action", "In the dark the cart is a long shape and the rain is "
                   "louder than it was."),
        ("character", "WREN"),
        ("dialogue", "Bring the second cart. And the good tarp, not the "
                     "one with the hole in it."),
        ("action", "Holt goes. Wren stands in the rain with a clipboard "
                   "and does not hurry."),
    ],
    [  # sheet 5: text only, so the logo is furniture on a sheet with no art
        ("scene", "INT. ARCHIVE ROOM - DAWN"),
        ("action", "The cart is gone. A single storyboard panel is still "
                   "pinned to the window."),
        ("character", "WREN"),
        ("dialogue", "Leave that one."),
        ("trans", "FADE OUT."),
    ],
]


def rgb_gradient(w, h, seed):
    """Deterministic synthetic pixels. Never a real photograph: fixtures are
    public and a real storyboard panel is somebody's confidential art.

    Blocky rather than smooth, and deliberately so. Eight-pixel runs give
    deflate something to match, which is what keeps this fixture tens of KB
    instead of hundreds; varying in BOTH axes is what keeps it able to catch
    a transposed or reversed row downstream, which a striped image could not.
    """
    px = bytearray()
    for y in range(h):
        row = bytearray()
        for x in range(w):
            row += bytes((((x // 8) * 37 + seed) % 256,
                          ((y // 8) * 53 + seed) % 256,
                          (seed * 3) % 256))
        px += row
    return bytes(px)


def image_xobject(w, h, seed):
    """A FlateDecode RGB image XObject body, ready for the object table.

    FlateDecode rather than DCTDecode so the generator stays pure stdlib and
    the bytes are reproducible. pdf.js hands both back as the same decoded
    RGB pixels, so the code path under test is identical either way.

    TRAP, if fixture-stability.test.ts ever fails on images.pdf alone: byte
    identity here rests on the HOST zlib producing the same deflate stream,
    and the format pins only what decompresses back out, not how. Any zlib
    build whose deflate output differs (zlib-ng, a forked or hardware-
    accelerated zlib) packs the same pixels into different bytes. So a 15KB
    binary inequality on this fixture and no other means suspect the
    compressor before the emitter -- decompress both sides and compare the
    PIXELS, which are what this file actually promises.

    What has been checked: images.pdf was regenerated byte-identically on
    Linux with zlib 1.2.13 / Python 3.11 on 2026-10-01; the Mac it was
    authored on is zlib 1.2.12.
    """
    data = zlib.compress(rgb_gradient(w, h, seed), 9)
    head = (
        b"<< /Type /XObject /Subtype /Image /Width %d /Height %d "
        b"/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode "
        b"/Length %d >>" % (w, h, len(data))
    )
    return head + b"\nstream\n" + data + b"\nendstream"


def place(name, x, y, w, h):
    """Content-stream operators drawing `name` at (x,y) with size (w,h),
    in PDF points with the origin bottom-left.

    An image XObject always occupies the unit square, so this `cm` matrix IS
    the placement the parser reads back out of the operator list.
    """
    return f"q {w:.2f} 0 0 {h:.2f} {x:.2f} {y:.2f} cm /{name} Do Q\n"


def xobject_bodies():
    """name -> XObject body, for every picture the images kind can draw."""
    return {name: image_xobject(*spec) for name, spec in IMAGES.items()}


def image_rows(items, sheet):
    """(kind, text) -> content_stream rows, with ("gap", n) reserving space.

    A cue or a parenthetical prints directly above what it introduces, as in
    flow_torture; everything else gets its blank line. `sheet` is carried
    only so the overflow message can say which one is too long.
    """
    rows = []
    for kind, arg in items:
        if kind == "gap":
            rows.extend([None] * arg)
            continue
        for chunk in textwrap.wrap(arg, WRAP[kind]) or [""]:
            rows.append((X[kind], chunk))
        if kind not in ("character", "paren"):
            rows.append(None)
    if len(rows) > LINES_PER_PAGE:
        # Raising rather than overflowing, for flow_torture's reason: rows
        # past the last one just draw at negative y, off the sheet, and the
        # fixture would still generate, still open, and still pass byte
        # stability while quietly testing less than it claims to.
        raise SystemExit(
            f"images sheet {sheet} needs {len(rows)} lines and a page holds "
            f"{LINES_PER_PAGE}. Text would run through the bottom margin and "
            f"off the sheet: shorten it, or move content to a new sheet.")
    return rows


def image_streams():
    # Sheet 1 is the title page and carries no scripted rows; sheets 2-5 are
    # IMAGE_SHEETS, numbered the way a script is (first page unnumbered).
    pages = [title_stream(IMAGE_TITLE)] + [
        content_stream(image_rows(items, i + 2), i + 1 if i else None)
        for i, items in enumerate(IMAGE_SHEETS)
    ]
    return ["".join(place(*p) for p in SHEET_PLACEMENTS[i]) + s
            for i, s in enumerate(pages)]


# --- the jpx kind --------------------------------------------------------
# One picture, in the one image format pdf.js cannot decode out of its own
# bundle. JPEG2000 (/JPXDecode) goes to a separate decoder module that pdf.js
# loads AT RUNTIME from the directory given by its `wasmUrl` option, and with
# no such directory every JPX image in a document fails at once -- silently,
# since a picture that will not decode is simply a picture the book does not
# have. A real 110-page script-plus-lookbook lost all 34 of its pictures that
# way. src/parser/pdfjs-wasm.ts is the fix; this fixture is what notices if it
# ever stops working.
#
# Deliberately its OWN kind rather than a seventh picture in `images`. The
# blast radius is the point: JPX either decodes or it does not, and when it
# does not, one small fixture should fail and say so, rather than a dozen
# assertions about rules that never came into it.
#
# The picture is 64x48 source pixels drawn at 180x135pt on a 612x792 sheet,
# which is imPanel's geometry from the images kind and clears every filter the
# same way: 5.0% of the page against the 3% area floor, 29% wide against 10%,
# 17% high against 3%, 64 and 48 pixels against the floor of 32. It sits on one
# sheet of two, and the furniture share needs a picture on at least two sheets
# before it can fire, so repetition cannot catch it either. It must come out
# KEPT, at 64x48, or the fixture is testing nothing.

# A JPEG2000 codestream, base64. NOT generated at run time, unlike every other
# picture here: a J2K encoder is a wavelet transform and an arithmetic coder,
# far past what this generator is allowed to be, and shelling out to one would
# make the fixture unbuildable on a machine that lacks it. So the bytes are
# committed, and here is exactly how they were made -- from THIS file's own
# rgb_gradient, so the pixels are the same invented blocks as every other
# fixture picture and can be regenerated by anyone who has openjpeg:
#
#     python3 -c 'import sys; sys.path.insert(0, "tools"); ...' > grad.ppm
#     # P6 header (64 48 255) followed by rgb_gradient(64, 48, 71)
#     opj_compress -i grad.ppm -o grad.j2k     # lossless, its default
#     base64 grad.j2k
#
# LOSSLESS matters and is not the only option opj_compress has: the test
# asserts the decoded pixels equal rgb_gradient(64, 48, 71) byte for byte,
# which is what proves the picture came through CORRECT rather than merely
# present. A lossy codestream would still decode, still be kept, and still look
# like a passing fixture while quietly checking nothing.
JPX_GRADIENT = (64, 48, 71)      # w, h, seed -- the pixels below, decoded
JPX_CODESTREAM = (
    "/0//UQAvAAAAAABAAAAAMAAAAAAAAAAAAAAAQAAAADAAAAAAAAAAAAADBwEBBwEB"
    "BwEB/1IADAAAAAEBBQQEAAH/XAATQEBISFBISFBISFBISFBISFD/ZAAlAAFDcmVh"
    "dGVkIGJ5IE9wZW5KUEVHIHZlcnNpb24gMi41LjT/kAAKAAAAAALjAAH/k8fUCg9l"
    "5RR/z7QQA5c568+0FAxdcMxpw+oEh9QHACEOBmssAccfB6PtAwAIhX/PwBY+0DAO"
    "BvBcLwiFf8D5A8PqCoA6IBZ+fTtHPb8Lbb+7ZsUn9v9/EBq5c6PtCwAMYHkFM8xy"
    "lJzJK8PqCY+0LBZ+fVF7vgo5vwxgeQUzzHKUnMkrwfOYh9Q3ACInOmS3/F9+AiwA"
    "lNHZlZOYWgJwKrblbPkDhLcbwoYxD1AW+Dw48rCigLbffEFpTFREyYciX6PtHgAA"
    "1K8bwoYxD1AW+Dw48rCip2ZmMWrxDAPEdI6yAL/H2jsfaPAnOmS4YBt69JKZQmod"
    "pn0e8SO/I+hrFAK5jWJTNwDUrxvChjEPUBb4PDjysKKnZmYxavEMA8R0jrIAv8Hz"
    "m4fUWwAnQOBB4a9g1KwNnsgkkMMPCylfE/A5sCOEoBalZq5EcevvBVE2aj3l57pP"
    "xAyxaUMuGs+0yUnugo4RA4c3oghQjAujOtjexI2vJAsCPZh9k6PtKwBnaqq49feC"
    "qJs1HvLz3SfwxqBM3r8lSlKuJs0g7JZ9DZGGbPzJXZYRGMy/x9pTH2lYQOBB4a9g"
    "1KwNnsgkkLoSEv4q+cB8ICs/ti8NgAAYKdv8sX6omhL7/39naqq49feCqJs1HvLz"
    "3SfwxqBM3r8lSlKuJs0g7JZ9DZGGbPzJXZYRGMy/wfOph9Q6dCrHPc4VEu5q1PvB"
    "EOQlYBgNTD8qoPi+iXnKc+MLrJGtvjC6yRrb4x/ENkqeb9YOxzS5O477fMgAAAAw"
    "jWH2QWPTEhL/f6PtJQDEMOqOb9YOxzS5O477fMgAAAAwjWH2QWPTEhLYpcAGEd8p"
    "hIS/x9plH2kodCrHPdDCim5q1PvBEOQlYBgNTD8qoPi9ShnNZ4Li2o/tR/aj+1Hw"
    "FvkJfjvoAABhIS/EMOqOb9YOxzS5O477fMgAAAAwjWH2QWPTEhLYpcAGEd8phIS/"
    "/9k="
)


def jpx_xobject():
    """The JPEG2000 image XObject body.

    No /ColorSpace and no /BitsPerComponent, which is not an omission: for
    /JPXDecode the PDF spec makes both optional because the codestream carries
    them, and leaving them out is what a real JPX-bearing PDF from a layout app
    looks like. It also means the fixture exercises the branch where pdf.js has
    to ASK the decoder how many components there are -- which it can only do if
    the decoder loaded, the whole point here.
    """
    import base64
    data = base64.b64decode(JPX_CODESTREAM)
    w, h, _ = JPX_GRADIENT
    head = (b"<< /Type /XObject /Subtype /Image /Width %d /Height %d "
            b"/Filter /JPXDecode /Length %d >>" % (w, h, len(data)))
    return head + b"\nstream\n" + data + b"\nendstream"


JPX_TITLE = [
    (4.0, "THE SECOND UNIT"),
    (4.5, "Written by"),
    (5.0, "J. Q. Nobody"),
]

# One scripted sheet. Thirteen reserved rows put the action a full line below
# the picture's bottom edge, exactly as sheet 2 of the images kind does.
JPX_SHEET = [
    ("scene", "INT. COLOUR TIMING BAY - NIGHT"),
    ("gap", 13),
    ("action", "A reference print hangs under the loupe, one frame of it "
               "circled in grease pencil."),
    ("character", "WREN"),
    ("dialogue", "The blue is two points cold and it has been all week."),
    ("action", "HOLT writes the number down without looking up."),
    ("character", "HOLT"),
    ("dialogue", "Then we time it again."),
]

JPX_PLACEMENT = ("imJpx", 108, 561, 180, 135)


def jpx_streams():
    pages = [title_stream(JPX_TITLE), content_stream(image_rows(JPX_SHEET, 2), 1)]
    return [pages[0], place(*JPX_PLACEMENT) + pages[1]]


def build(path, streams, xobjects=None, page_xobjects=None):
    objs, n_pages = [], len(streams)
    # Objects: 1 catalog, 2 pages, then one per font, then one per image
    # XObject, then page/content pairs. Every kind carries all four fonts
    # even when it uses one, so the resource dict is identical across kinds
    # and there is only one object-numbering scheme to reason about. Images
    # are the exception: which sheets list a picture is exactly what the
    # furniture rule reads, so each page names only the ones it draws.
    if xobjects is None:
        xobjects = {}
    # Two guards, for the two ways of leaving pages unaware of the images
    # they were handed. The failure they prevent is INVISIBLE: a page that
    # does not name an image still generates a valid PDF, every Do is a
    # no-op against a missing resource, and you get the right page count, the
    # right text, no pictures, and a byte-stability check that passes because
    # it compares against whatever was committed last. The first catches
    # xobjects given with page_xobjects left out altogether; the second
    # catches a page_xobjects list whose length differs from the streams.
    #
    # What they do NOT catch: a page_xobjects list of the right length that
    # simply never names some image, or a content stream whose Do names an
    # image its page's list omitted. Keeping those two consistent is the
    # caller's job, which is why XOBJECTS builds its lists from the same
    # placement tables the content streams are drawn from.
    if xobjects and page_xobjects is None:
        raise SystemExit("build: xobjects given with no page_xobjects; no "
                         "page would name them and every Do would be a no-op")
    if page_xobjects is not None and len(page_xobjects) != n_pages:
        raise SystemExit(
            f"build: {len(page_xobjects)} page_xobjects entries for "
            f"{n_pages} streams. Pages are walked by stream index, so a "
            f"longer list loses its tail without a word and a shorter one "
            f"dies on an IndexError that names neither list.")
    if page_xobjects is None:
        page_xobjects = [()] * n_pages

    names = list(xobjects)
    first_image_obj = 3 + len(FONTS)
    # name -> object number, so a placement naming an image that IMAGES does
    # not define fails as KeyError: 'imGhost' rather than a bare ValueError.
    image_obj = {name: first_image_obj + i for i, name in enumerate(names)}
    first_page_obj = first_image_obj + len(names)
    kids = " ".join(f"{first_page_obj + 2*i} 0 R" for i in range(n_pages))
    objs.append("<< /Type /Catalog /Pages 2 0 R >>")
    objs.append(f"<< /Type /Pages /Kids [{kids}] /Count {n_pages} >>")
    for key in FONTS:
        objs.append(f"<< /Type /Font /Subtype /Type1 /BaseFont /{FONTS[key]} >>")
    for name in names:
        objs.append(xobjects[name])
    res = " ".join(f"/{k} {3+i} 0 R" for i, k in enumerate(FONTS))
    for i, s in enumerate(streams):
        drawn = page_xobjects[i]
        xo = ""
        if drawn:
            xo = " /XObject << %s >>" % " ".join(
                f"/{d} {image_obj[d]} 0 R" for d in drawn)
        objs.append(
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.0f} "
            f"{PAGE_H:.0f}] /Resources << /Font << {res} >>{xo} >> "
            f"/Contents {len(objs)+2} 0 R >>")
        objs.append(f"<< /Length {len(s)} >>\nstream\n{s}\nendstream")

    # A bytearray, not a str, because a FlateDecode image body is arbitrary
    # binary. Everything else is still latin-1 text, and latin-1 is one byte
    # per character, so the offsets the xref records are unchanged.
    buf, offsets = bytearray(b"%PDF-1.4\n"), []
    for i, body in enumerate(objs, start=1):
        offsets.append(len(buf))
        buf += f"{i} 0 obj\n".encode("latin-1")
        buf += body if isinstance(body, bytes) else body.encode("latin-1")
        buf += b"\nendobj\n"
    xref = len(buf)
    buf += f"xref\n0 {len(objs)+1}\n0000000000 65535 f \n".encode("latin-1")
    for off in offsets:
        buf += f"{off:010d} 00000 n \n".encode("latin-1")
    buf += (f"trailer\n<< /Size {len(objs)+1} /Root 1 0 R >>\n"
            f"startxref\n{xref}\n%%EOF\n").encode("latin-1")
    open(path, "wb").write(bytes(buf))
    return n_pages


KINDS = {
    "screenplay": lambda: [title_stream()] + [
        content_stream(p, i + 1 if i else None) for i, p in enumerate(layout())
    ],
    "prose": lambda: [prose_stream()],
    "blank": lambda: [blank_stream(), blank_stream()],
    "images": image_streams,
    "jpx": jpx_streams,
}

# A second registry, because a picture is not expressible as a content stream
# alone: it needs an object of its own and a per-page resource entry naming
# it. kind -> (name -> XObject body, names drawn on each sheet).
XOBJECTS = {
    "images": lambda: (xobject_bodies(),
                       [[p[0] for p in sheet] for sheet in SHEET_PLACEMENTS]),
    # Sheet 1 is the title page and names nothing; the picture is on sheet 2.
    "jpx": lambda: ({JPX_PLACEMENT[0]: jpx_xobject()}, [(), (JPX_PLACEMENT[0],)]),
}


def _load_sibling(filename, modname):
    """Load a data file that sits beside this script. The content files are
    kept apart so the person writing screenplay never has to read layout
    code, and the dash in their filenames means they cannot be a normal
    import."""
    import importlib.util
    import pathlib
    path = pathlib.Path(__file__).with_name(filename)
    spec = importlib.util.spec_from_file_location(modname, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _torture_content():
    """tools/torture-content.py: 14 sheets of screenplay, as data."""
    return _load_sibling("torture-content.py", "torture_content")


def torture_streams():
    mod = _torture_content()
    pages = flow_torture(mod.CONTENT)
    return [title_stream(mod.TITLE)] + [
        torture_content_stream(p, i + 1) for i, p in enumerate(pages)
    ]


KINDS["torture"] = torture_streams


# --- demo: the invented feature the README and site pictures show ---------
# Laid out PAGE BY PAGE, not flowed: the site's scene has to land on printed
# pages 14 to 18, where the site's own page markers say it is. So each page
# is drawn exactly as tools/field-station-content.py writes it, and a page
# that does not fit is an error rather than a silent reflow that would move
# every page number after it.

def _demo_content():
    """tools/field-station-content.py: the Field Station pages, as data."""
    return _load_sibling("field-station-content.py", "field_station_content")


# A real script puts no blank line between a character name and what that
# character says, so neither does this. Everything else gets one above it.
_JOINED_TO_CUE = {"paren", "dialogue"}


def flow_page(rows):
    """One page of (kind, text) rows -> the (x, text) / None rows
    content_stream draws. Raises SystemExit if the page overflows."""
    out = []
    for kind, text in rows:
        if out and kind not in _JOINED_TO_CUE:
            out.append(None)
        # Straighten BEFORE wrapping, so the wrap measures what is drawn: an
        # em dash is drawn as two characters and would otherwise count as one.
        text = text.translate(ASCII_MAP)
        for chunk in textwrap.wrap(text, WRAP[kind]) or [""]:
            out.append((X[kind], chunk))
    if len(out) > LINES_PER_PAGE:
        raise SystemExit(
            f"field-station page overflows: {len(out)} lines, the limit is "
            f"{LINES_PER_PAGE}. It starts: {rows[0]!r}")
    return out


def demo_streams():
    mod = _demo_content()
    return [title_stream(mod.TITLE)] + [
        content_stream(flow_page(p), i + 1 if i else None)
        for i, p in enumerate(mod.PAGES)
    ]


KINDS["demo"] = demo_streams


# --- layout as data, for tests -------------------------------------------
# Page PLACEMENT is the thing most likely to rot silently: a speech written
# to start at line 50 so it straddles a page break still satisfies every
# output-shaped assertion at line 48, while proving nothing. These mirror
# what the content streams draw, so an assertion about a line number is an
# assertion about the actual PDF.

def _title_rows(table):
    out = []
    for inches_down, text in table:
        x = (8.5 - len(text) / 10.0) / 2.0
        out.append({"line": int(inches_down * 6), "x": round(x, 3),
                    "runs": [{"styles": [], "text": text}], "underline": False})
    return out


def _content_rows(rows):
    out = []
    for n, row in enumerate(rows):
        if row is None:
            continue
        x, text = row
        out.append({"line": n, "x": round(x, 3),
                    "runs": [{"styles": [], "text": text}], "underline": False})
    return out


def _torture_rows(rows):
    """Styled rows, including two-column ones, as flat data. Both columns of
    a dual line report the SAME line number, which is the property that
    makes them a dual region at all."""
    out = []
    for n, row in enumerate(rows):
        if row is None:
            continue
        pairs = row[1] if row[0] == "multi" else [row]
        for x, runs in pairs:
            out.append({
                "line": n,
                "x": round(x, 3),
                "runs": [{"styles": r["styles"], "text": r["text"]} for r in runs],
                "underline": any("u" in r["styles"] for r in runs),
            })
    return out


def layout_json(kind):
    if kind == "torture":
        mod = _torture_content()
        pages = [{"page": 1, "rows": _title_rows(mod.TITLE)}]
        for i, rows in enumerate(flow_torture(mod.CONTENT)):
            pages.append({"page": i + 2, "rows": _torture_rows(rows)})
        return pages
    if kind == "demo":
        mod = _demo_content()
        pages = [{"page": 1, "rows": _title_rows(mod.TITLE)}]
        for i, rows in enumerate(mod.PAGES):
            pages.append({"page": i + 2, "rows": _content_rows(flow_page(rows))})
        return pages
    if kind != "screenplay":
        # prose, blank, images and jpx have no line-addressable structure
        # worth emitting: one is a wall of paragraphs, one has no text
        # operators at all, and the two picture kinds are about where
        # pictures sit and what the filters make of them, not about text rows.
        return []
    pages = [{"page": 1, "rows": _title_rows(TITLE)}]
    for i, rows in enumerate(layout()):
        pages.append({"page": i + 2, "rows": _content_rows(rows)})
    return pages


if __name__ == "__main__":
    import sys
    argv = sys.argv[1:]

    if argv and argv[0] == "--atline-overflow-check":
        # Proves the guard fires: asked to pad back to line 2 having already
        # emitted well past it. Exits nonzero via SystemExit's message.
        flow_torture([("action", "x " * 200), ("atline", 2)])
        sys.exit("expected atline to raise, it did not")

    if argv and argv[0] == "--wrap":
        import json
        plain, spans = parse_markup(argv[2])
        print(json.dumps(wrap_spans(plain, spans, int(argv[1]))))
        sys.exit(0)

    if argv and argv[0] == "--parse-markup":
        import json
        plain, spans = parse_markup(argv[1])
        print(json.dumps({"plain": plain, "spans": spans}))
        sys.exit(0)

    if argv and argv[0] == "--emit-layout":
        if len(argv) != 2 or argv[1] not in KINDS:
            sys.exit(f"usage: make-fixture.py --emit-layout <{'|'.join(KINDS)}>")
        import json
        print(json.dumps(layout_json(argv[1])))
        sys.exit(0)

    if len(argv) != 2 or argv[0] not in KINDS:
        sys.exit(f"usage: make-fixture.py <{'|'.join(KINDS)}> <out.pdf>")
    kind, p = argv
    xobjs, page_images = XOBJECTS.get(kind, lambda: (None, None))()
    print(f"{build(p, KINDS[kind](), xobjs, page_images)} pages -> {p}")
