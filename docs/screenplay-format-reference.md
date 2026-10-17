# Screenplay format — technical baseline for reflowable conversion

Collected 2026-07-22 as the ground truth for Method Reader's CSS decisions and
the planned Mac app's formatting options. Two halves: what print format
actually is, and what Kindle's renderer actually honors.
Pagination — page-break rules, (MORE)/(CONT'D)/CONTINUED artifacts, and
what each tool emits — lives in `pagination-reference.md` (2026-07-30).

## 1. Print format (US feature standard)

12pt Courier (fixed pitch: 10 chars/inch, 6 lines/inch), single-spaced,
~55 lines/page, one blank line between elements. On 8.5" paper:

| Element        | From paper left | Width       | Notes |
| --- | --- | --- | --- |
| Page margins   | L 1.5", R 1.0"  | body = 6.0" | top/bottom 1" |
| Action         | 1.5"            | 6.0" (full) | flush left |
| Dialogue       | 2.5"            | ~3.5"       | ends ~6.0" |
| Parenthetical  | ~3.1"           | ~2.4"       | between cue and speech |
| Character cue  | 3.7"            | —           | NOT centered — fixed indent |
| Transition     | right-aligned   | —           | ends at 7.5" |
| Scene heading  | 1.5"            | full        | uppercase |

**Proportions relative to the 6" body column** (what a reflowable page
maps to):

- Dialogue column: starts at **17%** (1.0"/6.0"), width **~58%** (3.5"/6.0")
- Within the dialogue column: cue indented **+34%** of the column
  (1.2"/3.5"), parenthetical **+17%** (0.6"/3.5")
- Vertical rhythm: one blank Courier line between elements = **1em** at
  single spacing

Key perception note: print dialogue *looks* centered because the column
sits in the middle of the page, but the cue is a fixed indent within it,
not centered text. Reproduce with a narrowed block + internal indents,
never `text-align: center`.

## 2. What Kindle actually honors (KDP guidelines + field reports)

- **`max-width` / `max-height`: stripped** during conversion. Any layout
  relying on them (e.g. `margin: auto` + `max-width` centering) silently
  collapses to full width. ← This was Method Reader's full-width-dialogue bug.
- **Left/right margins & padding: use `%`** — Amazon's explicit
  prescription. Percentages scale with screen size, which is exactly the
  "various screen sizes" requirement.
- **Top/bottom margins: use `em`** — vertical rhythm scales with font
  size. (Matches what Method Reader already did.)
- **Line-height / font-size on body text: leave at defaults.** Enhanced
  Typesetting expects 1em/default line height and strips or fights
  overrides; the *reader's own* line-spacing setting owns this. So:
  within-paragraph line spacing is the device's knob, not ours —
  **between-element spacing (em margins) is our knob.**
- **`page-break-before/after/inside` + `break-*` with
  `avoid/auto/always`: supported.** Keep-with-next works, but every
  `avoid` link in a chain (heading→cue→parenthetical→dialogue) enlarges
  the unbreakable chunk a renderer must push to the next page — pushed
  chunks read as "weird page breaks" / blank space at page bottoms.
  Use the minimum chain that protects reading order.
- Negative margins: unsupported. Floats in tables: unsupported. Fixed
  units (pt/px): avoid throughout.

## 3. Translation table (what Method Reader now emits)

| Print fact | Reflowable rule |
| --- | --- |
| Dialogue col at 17% → 58% wide | `.dialogue-block { margin-left: 20%; margin-right: 20%; }` (symmetric — screens have no binding edge) |
| Cue +34% into column | `p.character { margin-left: 33%; }` (% of block) |
| Parenthetical +17% into column | `p.parenthetical { margin-left: 17%; margin-right: 8%; }` |
| One blank line between elements | `1em` top/bottom margins in em |
| Single-spaced within element | no `line-height` override — device default |
| Cue stays with speech | `break-after: avoid` on cue only |
| Heading stays with scene | `break-after: avoid` on heading |
| Transitions right-flush | `text-align: right` |

Rationale for symmetric 20% (vs print's asymmetric 17%/25%): print's
extra left margin exists for binding holes; on screen, symmetric margins
read as the "centered column" writers expect. 20% yields a 60% column —
within a point of print's 58%.

## 4. Open items to verify on hardware

- Whether `%` margins inside a `%`-margined parent compound acceptably
  at phone widths + max font size (worst case: cue wraps). If so, the
  app's cue-indent option may need a phone-profile preset.
- Whether pushed-chunk gaps persist with the minimal avoid chain. (The
  toggle half of this item shipped: `keepSceneHeadingWithScene` exists
  and defaults ON — registry #5a. The e-ink pushed-chunk observation
  itself is still unverified on hardware; 2026-07-30 note: keeps only
  bind on KFX anyway, sideloaded AZW3/MOBI ignore them — registry #8b.)

Sources: [KDP Text Guidelines (reflowable)](https://kdp.amazon.com/en_US/help/topic/GH4DRT75GWWAGBTU),
[KDP Enhanced Typesetting](https://kdp.amazon.com/en_US/help/topic/G202087570),
[max-width stripped — field report](https://github.com/rupor-github/fb2mobi/issues/22),
[Quote-Unquote Apps screenplay format guide](https://blog.quoteunquoteapps.com/standard-screenplay-format-the-writers-guide/),
[Celtx screenplay margins guide](https://blog.celtx.com/screenplay-margins-guide/),
[Final Draft format reference](https://www.finaldraft.com/learn/how-to-format-a-screenplay/).
