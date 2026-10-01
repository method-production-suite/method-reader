# Formatting decisions log — future Mac app options registry

Every formatting behavior Screepub applies, logged as it was tuned
(2026-07-22, initial build + two feedback rounds on a real script). The print
geometry and Kindle CSS constraints behind these choices live in
`docs/screenplay-format-reference.md` — read that first when adjusting. Each entry is
written as a **toggle or slider in the Mac app** (the reader window's rail;
2026-07-22, moved out of Settings 2026-07-30): the engine's FormatOptions
(`src/options.ts`) is the single knob surface, reachable via
`--options file.json` on the CLI and mirrored by `FormatSettings` in the
app. Entries below marked "not built" remain future work; everything else
is live.

**Where a conversion's options come from (2026-09-14):** explicit flag
(`--options` / `--options-json`) over the script's sidecar
(`<Stem>.screepub.json`, written by `screepub settings`) over these
defaults, knob by knob, through the one merge `resolveFormatOptions` already
is. The conversion itself reads the sidecar — beside the input, or in the
script's library folder under `--library`, which wins — so a tuned script's
FIRST book carries its tuning. It used to be read only by `screepub
settings`, so the library adopted a sidecar and then rendered at the
defaults; every export rung but MOBI converts that EPUB as it stands, so
those defaults reached the reader. An unusable sidecar is ignored (the
conversion still succeeds) and, either way, the engine says on stderr which
file it used and names it as `settingsPath` in the `--json` answer.

**App defaults now sit in that chain too (2026-09-23):** one more layer,
between the script's sidecar and these shipped defaults, holds the app-wide
format defaults set with `screepub app-settings`. The precedence: explicit
flags > the script's saved settings > app defaults > these shipped
defaults. The desktop window's per-script Settings page reaches the same
layer from its foot: "Use these for new scripts" promotes that script's
current settings to the app defaults, and "Reset new scripts to Screepub's
defaults" (shown only when the two differ) puts the shipped defaults back.
The first `--library` conversion of a PDF with no sidecar of its own saves
the settings it started from as that script's own, so a later change to
the app defaults reaches only scripts converted afterwards; a script
already in the library from before this change has no saved settings, and
keeps following the app defaults until it is tuned once.

**That save is a choice (2026-09-24):** the Settings page's "When a PDF is
converted" offers "Keep its settings" (the default, the behaviour above)
and "Follow the defaults", stored as `keepScriptSettings` in the app
settings file and written with `screepub app-settings --set
'{"keepScriptSettings":false}'`. Off, a library conversion saves nothing,
so a script nobody has tuned keeps following the app defaults. Switching
reaches only conversions from then on: a script that already has a
sidecar keeps it, because a saved one and a tuned one are the same file
and the engine never deletes or rewrites either because of this choice.
Spec: `docs/superpowers/specs/2026-09-24-keep-script-settings-choice-design.md`.

**Coverage of these entries by the committed torture fixture is tracked in
`tools/torture-manifest.json`**, one row per entry, and
`tests/torture-coverage.test.ts` fails when an entry has no decision
recorded there. A row saying `"covered": false` with a reason is a
decision; silence is not. So adding an entry below means adding its row,
and the suite will say so if you forget.

## Layout & flow

### 1. Continuous scene flow (no page break per scene)
- **What:** scenes are anchored `<section>`s packed into one body XHTML
  file; files split only past a size budget, at a scene boundary. Spine
  boundaries force page breaks in every reader — one-file-per-scene made
  every slugline start a fresh page (the first feedback-round complaint).
- **Default:** continuous, 250 KB/file budget.
- **App option:** toggle "Start each scene on a new page" (off) +
  advanced: file-size budget.
- **MOBI arm (2026-07-30):** the MOBI dialect has exactly one fragmentation
  primitive, `<mbp:pagebreak/>`, and `scenePageBreaks` now drives it: one
  break before each PRIMARY scene heading. Mini-slugs are excluded,
  matching the EPUB, which opens a `section.scene` for primary slugs only
  (#5b). What that dialect cannot reach is the CSS knobs: it ships no
  stylesheet (#8c), so every keep, margin and alignment option is EPUB
  territory. Stage-1 knobs (#8, #8a, #10a's mode, #13a's markers) arrive
  already baked into the `.fountain`. The other stage-2 knob it does read
  is `includeTitlePage` (#11).
- **Corrected 2026-07-31:** the break was suppressed before the FIRST
  scene heading, which is right when the book opens on a scene and wrong
  when anything precedes it — a script opening on action ran that action
  into scene one, where the EPUB separates them. The gate is now "has any
  body content been emitted since the last break", so the title page's
  own break (or the start of a title-less book) still leaves no blank
  leading page.
- **Code:** `src/epub/html.ts` (`tokensToBody`, `DEFAULT_MAX_FILE_BYTES`),
  `src/mobi/html.ts` (`scene_heading` case).

### 2. Centered dialogue column — % geometry (v2)
- **What:** dialogue is a narrow column with **percentage side margins**
  (20%/20% → ~60% column, print is 58%); cue indents **+33%** of the
  column and parenthetical **+17%** (print +1.2"/+0.6" of the 3.5" col).
- **History:** v1 used em left-indents (hugged left — wrong); v1.5 used
  `margin: auto` + `max-width: 21em` — looked right in browsers but
  **Kindle strips `max-width`**, so dialogue ran full width on device
  (the "runs the full width" bug). Amazon's own guidance: horizontal
  margins in `%`, vertical in `em`. Never reintroduce max-width.
- **Default:** 20% side margins; cue +33%; paren +17%/8%.
- **App options:** column width (as side-margin %), cue indent %, paren
  indent %; per-device presets (phone may want a shallower cue indent —
  see reference doc §4).
- **Code:** `src/epub/css.ts` (`.dialogue-block`, `p.character`,
  `p.parenthetical`).
- **AZW3 route (2026-07-22):** Calibre's remove-fake-margins heuristic
  reads per-block side margins on most paragraphs — i.e. a screenplay's
  dialogue column — as publisher page margins and deletes them (any
  unit, % or em), collapsing dialogue to full width on device (user
  report: "left justified, full horizontal space"). An `--extra-css`
  em fallback was no fix: on multi-file EPUBs Calibre attaches extra
  CSS only to its generated inline ToC. Cure is
  `--disable-remove-fake-margins`, which lets the EPUB's own % geometry
  (and the `dialogueSideMarginPct` knob) flow through to AZW3
  unchanged. Guarded by the kit-check "AZW3 keeps dialogue-block side
  margins" check. Code: `app/Sources/ScreepubKit/EbookConvert.swift`.

### 2b. Cue & parenthetical alignment (option, default centered)
- **What:** print's fixed cue indent only *optically* centers names on
  paper; reflowed to arbitrary screen widths it drifts left (user report
  2026-07-22: "centered items drooping left"). Default is now
  `text-align: center` within the dialogue column; `cueAlignment:
  'indented'` restores the % offsets (sliders apply only in that mode).
- **Code:** `src/options.ts`, `src/epub/css.ts`; app picker in the reader
  rail's Dialogue group.

### 3. Vertical rhythm between elements
- **What:** a full blank line (`1em`) between action paragraphs, dialogue
  blocks, transitions, centered text; bumped from the original tighter
  0.8em (a later feedback-round complaint). Inside a dialogue block
  (cue → paren → lines) spacing stays at 0 — that tightness is correct.
- **Default:** 1em between elements; scene heading 1.6em above / 0.8em
  below; mini-slug 1.4em above / 1em below (#5b).
- **App option:** "Element spacing" slider (compact 0.6 → airy 1.4em);
  possibly independent scene-heading spacing.
- **Code:** `src/epub/css.ts`.

### 4. Relative-unit scaling (principle, not an option)
- **What:** no fixed units anywhere — **horizontal structure in `%`**
  (scales with screen width; Kindle's prescription) and **vertical rhythm
  in `em`** (scales with font size). This split is the core fix over
  consumer converters — never expose an option that reintroduces
  pt/px/inches.

### 5a. Scene heading keeps its context (keep-with-next chain)
- **What:** `page-break-after: avoid; break-after: avoid` on
  `h2.scene-heading`. The heading is chained to whatever follows it, so a
  slugline never strands alone at a page bottom with its scene overleaf
  (user-requested 2026-07-22). Gated by `keepSceneHeadingWithScene`; with
  the option off the heading carries no avoid link at all.
- **Changed 2026-07-30 — wrapper deleted, chain gated:** the heading had
  BOTH mechanisms. The chain above was already in the stylesheet and
  applied unconditionally; on top of it, v1 also wrapped the heading and
  the scene's FIRST BLOCK in a `<div class="keep-together">` with
  `page-break-inside: avoid`. The wrapper is what broke: it made the
  heading plus an entire block — a whole action paragraph, or a whole
  speech — one unbreakable chunk, and a renderer that honors keeps pushes
  a chunk that does not fit *whole*. On KFX (sideloaded and
  Send-to-Kindle server conversions alike, see #8b) and in Apple Books,
  that is exactly what happened: pages ended half empty (the observed
  bug). So the wrapper was deleted and the surviving chain was newly put
  behind `keepSceneHeadingWithScene`, which until now gated only the
  wrapper.
- **Consequence of the gating:** with the toggle OFF, a heading now
  carries no keep at all — before, it always kept the chain and the
  toggle removed only the wrapper. Turning the option off is therefore a
  real loss of behavior, not a return to a neutral state.
- **The claim this traded away:** v1 added the container form because KDP
  documents it ("headlines with paragraphs to keep together") and it was
  believed *more reliably honored* than `break-after: avoid` alone.
  Dropping it is a knowing trade: the container's claimed robustness for
  bounded chunks. Device verification settles it — Kindle Previewer/KFX,
  plus an Apple Books pass: does the heading actually travel with the
  line beneath it? If `break-after: avoid` proves not to bind, the
  recorded fallback is a wrapper holding the heading + the first ELEMENT
  only — never the whole first block, which is the bug this replaced.
- **Device verdict 2026-09-15, Apple Books: DOES NOT BIND there.** An
  `h2.scene-heading` sat alone at the foot of a column with its action in
  the next one, with all three spellings of the rule present. See #8b for
  the full verdict and why the rule stays anyway. This does not disturb the
  Kindle verdict below — it is a second engine answering differently, which
  is the normal case, not a contradiction.
- **Device verdict 2026-07-30: BINDS — the chain holds without the
  wrapper.** First isolated evidence (nothing prior could tell the two
  mechanisms apart: #8b's 2026-07-29 pass measured cues wearing both):
  a real script sent via Send-to-Kindle web (Amazon server conversion →
  KFX, Enhanced Typesetting) read on device — no heading stranded at any
  page bottom, and page fill is markedly improved over the wrapper era
  (the half-empty-pages complaint that drove the rewrite). Evidence
  grade: one script, one route, owner-observed; a stranded heading on
  any future script reopens this, and the fallback (wrapper holding
  heading + first ELEMENT only) stays recorded above.
- **Default:** on.
- **App option:** "Keep headings with scene" toggle (the reader rail's
  Page group).
- **Code:** `src/options.ts` (`keepSceneHeadingWithScene`),
  `src/epub/css.ts` (`headingKeep` → `h2.scene-heading`).

### 5b. Mini-slugs (secondary sluglines) — micro-headings, not scenes
- **What:** "LATER", "IN THE BACK ROOM", "END OF MONTAGE" — the slugs that
  move you inside a scene. They render as `p.mini-slug`: bold, uppercase,
  1.4em above / 1em below (#3), `break-after: avoid`. NOT a scene: no
  `<section>`, no TOC entry (#12), no `scenePageBreaks` page break, and no
  5a keep-together wrapper.
- **How it survives the .fountain:** as Fountain's **forced slugline**
  (`.LATER`) — the format's own word for a heading that doesn't open
  INT./EXT., and what screenwriters type by hand. Plain text does not
  survive: bare "LATER" re-parses as action, and "BACK TO:" re-parses as
  a right-flush *transition*. `p.mini-slug` was styled from the first
  commit but no path ever emitted the class, because serialize.ts wrote
  mini-slugs plain — fixed 2026-07-30 by forcing the dot. Mini-slug text
  stays PLAIN (no emphasis markers), same rule as cues and parentheticals.
  A **leading dot** is the one thing the force can't carry (fountain-js's
  rule is `^\s*\.(?!\.+)`), so ".45 ON THE COUNTER" falls back to forced
  action; every other marker rides through the dot intact. The cue chain
  resets here as it does at a scene or transition — a mini-slug is a cut in
  time or place, so the next speaker never inherits a (CONT'D) (#8a).
- **Renderer side:** a `scene_heading` token whose text fails fountain-js's
  own unforced-heading pattern (`PRIMARY_SLUG` — a copy of its
  `rules.scene_heading` first alternative) could only have come from the
  forced form, so that is the exact discriminator. A numbered mini-slug
  (`.LATER #5#`) keeps its number exactly as a slugline does. MOBI has no
  third weight in its dialect: mini-slug and slugline are both `<p><b>`.
- **Classification must agree (2026-07-30):** `classify.ts` carries the
  same `PRIMARY_SLUG` literal and excludes it from priority 9, replacing
  the narrower `SCENE_HEADING`. Otherwise the two ends disagree: "EST. THE
  HOUSE" or "I/E CAR" classified as a mini-slug, serialized as `.EST. THE
  HOUSE`, and promoted straight back to a full scene heading with a TOC
  entry. `tests/epub.test.ts` pins the two literals to each other and the
  renderer's verdict to fountain-js's own tokenizer, so an upstream rule
  change fails a table instead of quietly reshuffling headings. The literal
  is anchored, so classify.ts also tests it against the line with a leading
  shooting-script number stripped — an unpaired number ("2 EXT. WOODS - DAY
  3", which the dual-margin strip leaves whole) pushes the opener off `^`.
- **THE TRADE — hand-written Fountain (2026-07-30):** Screepub owns the
  dot-force as its mini-slug carrier, so a hand-written `.BLACK` or
  `.THE BRIDGE` from another tool renders as a **mini-slug, not a scene**:
  bold line, no section, no TOC entry. A dot-forced line that also matches
  a real slug opener (`.INT. …`, `.EST. …`) still promotes. This is a cost
  accepted, not a feature: Fountain has exactly one forcing character for
  headings and two things need it. Recorded in the README's Fountain-input
  divergence table. Building a marker to tell the two apart was considered
  and rejected — any marker either pollutes the durable `.fountain` for
  other tools or hides the line from them entirely.
- **Classified from the PDF (2026-07-30) — this CLOSES the KNOWN GAP this
  entry carried,** and nothing above it changes: the dot-force, THE TRADE
  and the PRIMARY_SLUG agreement all still hold, with the classifier now
  the fourth thing obeying them. The gap was that the old branch
  asked for `indent < 5` on a measure taken from the PAGE edge, where the
  modal action indent is 15–18 — so no block in any generator ever scored
  under 5 and PDF input produced **zero** mini-slugs. The rule now works in
  the real action band and leans on shape instead. A block is a mini-slug
  when ALL hold: `indent < ACTION_MAX` (20); **exactly one line** in the
  block; strictly all-caps; 2–55 characters; letters are ≥50% of the
  non-space characters; the last character is a letter, digit or `:`; and
  it does not end in `TO:`. `PRIMARY_SLUG` (with and without a leading
  shooting-script number) and bare parentheticals are out.
- **Why each guard:** *single line* — groupBlocks breaks on a >20pt Y gap,
  so a one-line block IS a standalone beat, and prose long enough to be a
  sentence wraps. *Strict caps* (not the cue heuristic's 0.8 ratio) drops
  the "Dev CROSSES TO THE COUNTER" emphasis style. *Letters ≥50%* keeps a
  WGA registration number off the heading. *Terminal punctuation* is THE
  discriminator: across all six fixtures every sound effect and shouted
  beat carried a `.` or `!` and every true slug ended bare or on `:`, so
  the ambiguity the type invited (a bare "SILENCE" beat vs a bare "END
  DREAM" slug) does not occur in the corpus — it also rejects a trailing
  dash and a `***revision note***`. *Ends in `TO:`* is Fountain's own
  transition rule, and it is what keeps left-flush "DISSOLVE TO:" /
  "SMASH CUT TO:" out (the transition branch only fires right-flush).
  *`PRIMARY_SLUG`* is the shared heading exclusion above; testing it a
  second time against a number-stripped copy catches the unpaired
  shooting-script heading ("2 EXT. WOODS - DAY 3") that the anchor misses.
- **Priority stays 9 — LAST, after every existing veto.** Precision beats
  recall here: a missed slug renders as action (harmless), a promoted
  sound effect renders as a bold heading (visible defect). **The priority-7
  decision: `isActionByPattern`'s ACTION_PRONOUNS veto keeps its say, so
  "THE KITCHEN"-shaped slugs stay action.** The carve-out was measured and
  rejected: it would have gained ~18 true slugs in one fixture, but that
  same fixture holds "THE ENGINE STARTS." and "THE DOOR OPENS." — all-caps
  prose sentences one dropped period away from promotion. The shapes are
  identical; only the period separates them, so the second guard is free
  precision and the recall loss is deliberate. ACTION_CAMERA likewise still
  vetoes "FADE IN:" and "CLOSE ON: …".
- **Tested against the danger set:** "THUD.", "CRASH!", "DING!",
  "BEEP. BEEP.", an all-caps action sentence, a dash-broken line, a
  revision-starred note, both `TO:` transitions, mixed case, a wrapped
  two-line block, an over-long line, a digit-dominated registration line,
  and cue/speech-band non-interference (the band test alone forbids it:
  ACTION_MAX 20 sits below DIALOGUE_MIN 25 and CHARACTER_MIN 35).
- **Fixture sweep (5 generators + both committed fixtures):** total element
  count, and the scene / character / dialogue / parenthetical / transition /
  page-number counts, are **unchanged everywhere**; the only movement is
  action → mini-slug — 79 / 7 / 4 / 2 / 0 / 0, plus 2 section headings in
  the non-screenplay prose fixture (which still trips the guard, since that
  keys on scenes and dialogue). epubcheck clean on the 79-slug output.
- **The recurrence suppressor — mini-slugs go through the SAME rule as
  action.** Classification runs BEFORE `suppressBoilerplate`, so every line
  the mini-slug rule claims leaves the recurrence layer's view. Two risks
  came out of that, and the trail is worth keeping:
  - **Escape (real, fixed).** A left-flush all-caps per-page watermark
    ("CONFIDENTIAL", "PROPERTY OF THE STUDIO") is exactly mini-slug-shaped
    and recurs by definition. Outside the layer it renders as a bold
    micro-heading on every page — reproduced on a 10-page document: ten
    headings where the pre-classification parser suppressed them.
  - **Deletion (theoretical, measured at zero).** Put mini-slugs back in
    and a legitimately repeated slug that crosses the ≥40%-of-pages
    threshold gets hidden. Measured across the corpus: the widest
    page-spread of any mini-slug is chromium's 15 of 106 pages — **14%
    against a 40% bar** (final-draft 2%, fade-in 1%, highland 2%). It is
    not close, and that is structural rather than luck: a real slug family
    varies by location ("IN THE KITCHEN", "IN THE DRIVEWAY"), so it never
    recurs VERBATIM on 40% of pages. Text that does is watermark-shaped.
    One caveat for short documents: the threshold is
    `max(3, ceil(pageCount * 0.4))`, so under eight pages the real bar is
    3 pages, not 40% — a "LATER" on 3 pages of a 6-page short would be
    hidden. Pre-existing floor, shared with action; noted, not special.
  - **Resolution:** `mini-slug` is a recurrence candidate and suppresses to
    `page-number`, identical to `action`. Counting them keeps the pool
    exactly what it was before the classifier existed; hiding them on the
    same terms restores the pre-classification watermark behavior verbatim.
  - **Rejected variant — demote to `action` instead** (heading stripped,
    words kept). It reads well as a never-delete principle and fails twice.
    A watermark that types `action` on some pages and `mini-slug` on others
    comes out **incoherent**: hidden where it typed one way, visible where
    it typed the other, same mark, same document. And even in the pure case
    it is a **visibility regression** — recurrence-confirmed watermarks are
    hidden today, and demoting to action would start showing them. Hiding
    confirmed recurrence is this layer's proven job, not a hazard to
    engineer around.
  - **Residual:** a mini-slug that genuinely recurs verbatim on ≥40% of a
    script's pages would be hidden. Nothing in the corpus approaches it,
    and the pattern layer still runs first (priority 1.5), so revision
    slugs, draft stamps and dates can never become mini-slugs at all.
  - One uniform sink keeps `suppressBoilerplate` pure and **idempotent**:
    a suppressed mini-slug lands on `page-number` and stops there.
- **Known residual — transitions:** a left-flush transition that is neither
  camera-prefixed nor `TO:`-terminated ("SMASH TO BLACK", "OVER BLACK:")
  reads as a mini-slug. It renders at the right visual weight for what it
  is, so this is a naming miss, not a defect; 3 instances in one fixture.
- **Known residual — the colon is not always a colon:** one generator emits
  U+A789 (MODIFIER LETTER COLON), not ASCII `:`. So "every true slug ends
  bare or on a colon" holds only where extraction hands back ASCII, and the
  three `INTERCUT`/`CLOSE ON` slugs in that fixture fall to action today.
  That is the safe direction, and widening `MINI_SLUG_TAIL` to accept the
  lookalike would need its own evidence pass first.
- **Why `)` is not a slug ending:** the tail class is `[A-Z0-9:]`. A closing
  paren was in it briefly and admitted nothing: the only action-band lines
  that end on one are whole parentheticals, which have to be refused, and
  no true slug in any fixture ends on a paren. Dropping it also retired a
  redundant `PARENTHETICAL` guard the tail already subsumed.
- **Code:** `src/fountain/serialize.ts` (`mini-slug` case),
  `src/fountain/slug.ts` (`PRIMARY_SLUG`, `isMiniSlug` — the stage-2
  definition, moved here 2026-07-30 when MOBI became its second consumer),
  `src/epub/html.ts` and `src/mobi/html.ts` (both import it; the MOBI side
  uses it to keep `<mbp:pagebreak/>` off mini-slugs, #1), `src/epub/css.ts`
  (`p.mini-slug`), `src/parser/classify.ts` (`PRIMARY_SLUG`,
  `isMiniSlugShaped`, priority 9), `src/parser/boilerplate.ts`
  (`suppressBoilerplate` — mini-slug as recurrence candidate).

### 5. Keep-with-next — minimal chain (v2)
- **What:** `break-after: avoid` on the scene heading (gated by #5a's
  `keepSceneHeadingWithScene`), mini-slugs (live as of #5b — the rule
  predates its emitter), and the character cue (always on). v1 also
  chained parentheticals; every avoid link grows the unbreakable chunk a
  renderer pushes to the next page, and pushed chunks show up as
  occasional blank-bottom "weird page breaks."
- **Default:** heading + mini-slug + cue avoid; parenthetical breaks
  freely.
- **App options:** none — cue avoid stays always-on; the heading behavior
  is governed by 5a's CSS chain (gated by `keepSceneHeadingWithScene`).
- **Column-spelling shadow rule (2026-07-30):** every inside-avoid
  WRAPPER also carries `-webkit-column-break-inside: avoid`, in a
  SEPARATE declaration block (separate because iBooks drops both
  spellings when they share one), which extends those keeps to **Apple
  Books** — which honors only the old spelling — and to the **Readium
  family** (Thorium, Kobo's mobile apps). Kobo's kepub e-ink renderer is
  the hoped-for third, not a claimed one: it paginates with multicol, so
  the old spelling might reach it, but the only record we have says it
  ignores break CSS (device-map §6, MobileRead t=346874).
- **The list is DERIVED, not maintained twice (corrected 2026-07-31):**
  the shadow rule's selectors come from the same gating that emits the
  keeps (`columnKeeps` in css.ts) — #10a's `table.dual-dialogue`, and #8c's
  `.dialogue-block` when `keepSpeechesWhole` is on. #8b's wrapper was the
  third member until 2026-09-14; with it deleted the rule shares a selector
  with the table rule it shadows and is emitted AFTER it, so `ruleFor()`
  answers with the real keep rather than the column spelling. It first shipped as a hand-written list of
  the first two, which silently left the whole-speech keep inert in
  Apple Books: the toggle did nothing there while the cue keep worked.
  A keep can no longer join the inventory and skip the shadow rule.
  The keep-with-*next* chains in this entry still get no shadow — we
  emit no column-spelling `break-after` — so they stay modern and
  legacy-prefixed only.
- **Code:** `src/epub/css.ts`.

### 6. Typeface & line height (v2)
- **What:** `"Courier Prime", "Courier New", Courier, monospace`. The v1
  `line-height: 1.45` override is REMOVED — Enhanced Typesetting expects
  default line height on body text and the reader's own line-spacing
  setting owns within-paragraph spacing.
- **App options:** font menu only. Do NOT offer a line-height slider for
  Kindle targets — it would silently not work; between-element spacing
  (option 3) is the honest spacing knob we control.
- **Code:** `src/epub/css.ts` (`body`).

### 6b. Body text alignment — ragged-right by default (option)
- **What:** action and dialogue paragraphs are emitted with an explicit
  `text-align`. Screenplays are traditionally ragged-right (left
  aligned); some e-readers justify body text by default, opening
  distracting stretchy word gaps (observed on device 2026-07-22 —
  action lines stretched full-width). Default `justifyText: false`
  emits `text-align: left`, which overrides the reader's justification;
  `justifyText: true` emits `text-align: justify` for readers who
  prefer it. Cue/parenthetical alignment is governed separately (#2b);
  this knob is body text only.
- **Default:** ragged-right (`justifyText: false`).
- **App option:** "Justify body text" toggle (the reader rail's Text
  group).
- **Apple Books needs the OPF meta (2026-07-30):** unless the package
  carries `<meta property="ibooks:specified-fonts">true</meta>`, a Books
  reader with Justify switched on overrides our ragged-right `text-align`
  outright (and our font-family with it), so this knob simply does not
  hold there. Shipped 2026-07-30, with the `ibooks:` prefix declared on
  `<package>` so epubcheck stays quiet; every other reading system
  ignores the meta (device-map §3).
- **Code:** `src/options.ts` (`justifyText`), `src/epub/css.ts`
  (`bodyAlign` → `p.action`, `p.dialogue`), `src/epub/build.ts` (the OPF
  meta).

### 16. Transitions never begin a page (always on)
- **What:** `page-break-before: avoid; break-before: avoid` on
  `p.transition`. CUT TO:, DISSOLVE TO:, SMASH CUT TO: belong to the shot
  they end, not the one they introduce — a transition may sit at the
  bottom of a page, but must never be the first line of the next one,
  stranded above a slugline it has nothing to do with. That is the
  universal print rule rather than a house preference, so it is
  unconditional: no option, no gate.
- **Cost:** unlike the keep-with-next chains (#5, #5a), an avoid-*before*
  does not grow a forward chunk. The renderer moves the break earlier
  instead, and the element above a transition is normally a breakable
  action paragraph — so one line travels down with the transition. It
  only gets expensive when what precedes is itself unbreakable (a whole
  speech under #8c), in which case that block moves too.
- **Reference:** `docs/pagination-reference.md` §2, the break rules by
  element (that doc lands from branch `worktree-device-map`).
- **Device verdict 2026-08-03: HOLDS — no transition begins a page.**
  Deliberate look, sideloaded KFX on the Kindle, torture fixture sheet 2
  (CUT TO:, DISSOLVE TO:, SMASH CUT TO:). Worth recording that this is the
  FIRST time the rule was ever exercised on a device: transitions sat at
  the action margin in both committed fixtures until 2026-08-03, below
  TRANSITION_MIN, so every earlier pass was looking at action elements that
  merely read like transitions.
  The verdict lands in this entry.
- **Code:** `src/epub/css.ts` (`p.transition`).

### 17. Print split minimums (option, default ON; 2026-07-30)
- **What:** `printSplitMinimums` emits `widows`/`orphans` on `p.dialogue`
  and `p.action`: ON = 2 (print's two-line rule at page edges,
  pagination-reference §2), OFF = 1 (tight packing, the community's
  documented space-reclaim trick).
- **Device support:** KFX honors widows/orphans from fw 5.12.3 (Kindle
  Previewer 3.35 added them ~2019); RMSDK (Kobo epub, tolino) honors
  book CSS (MobileRead t=328903). Ignored: KF8/AZW3, MOBI. Unverified
  on kepub e-ink (patch-lore says its WebKit reads them; not confirmed
  on device). Apple Books: WebKit implements the properties; untested.
- **Interaction (load-bearing; rewritten 2026-09-14):** #8b's keep used to
  be a wrapper holding cue + parentheticals + the FIRST dialogue paragraph
  in `break-inside: avoid`, so a single-paragraph speech never split and its
  widows/orphans never fired. That wrapper is gone: #8b is now a chain on
  the cue and the parenthetical, nothing wraps the dialogue, and this rule
  therefore fires on EVERY dialogue paragraph including the first. It is
  what bounds the chunk the chain pushes. With #8c also
  ON, whole speeches are atomic and the dialogue arm is fully inert;
  the action arm is unaffected in every mode.
- **App option:** "Print-style split minimums" (reader rail, Page group).
- **Device verdict 2026-08-03: HOLDS — no stranded single lines.**
  Sideloaded KFX on the Kindle, torture fixture sheets 9-14: 40 numbered
  speeches, lengths cycling 1/2/4/3 lines, so breaks land inside speeches
  rather than only between them. No single line of dialogue or action was
  left alone at the top or bottom of any page.
  **Scope, stated so nobody over-reads it:** the 4-line speeches are the
  ones that carry this verdict, since a 1-2 line speech moves whole under
  #8b's keep and would validate the wrong rule. They are single-paragraph,
  so this confirms widows/orphans WITHIN a block; a multi-paragraph speech
  spanning a break is still unproven. OFF (1+1) was not tested, and the
  format matters: KFX only. AZW3 and MOBI ignore these properties, so a
  sideload in either format would look identical whether the rule fired or
  not.
- **Code:** `src/options.ts`, `src/epub/css.ts`, app FormatSettings +
  ReaderRail.

## Cleanup & rejoining

### 7. Right-margin revision stars — dropped
- **What:** star-only text items at >80% page width are production markup
  (revised-draft line markers). Dropped at extraction; leaving them in
  created bogus action elements that **reset dialogue context and
  fragmented speeches** (found via feedback-round testing 11.10.25: a
  character's dialogue lines). Author-written `*emphasis*` at text
  indents is untouched.
- **Default:** drop.
- **App option:** "Revision marks: hide / show right-aligned gutter ✱" —
  showing them needs a serializer+CSS path (float-right span), not built.
- **Code:** `src/parser/extract.ts` (`revisionMarginX`).

### 8. (MORE) / (CONT'D) page-break rejoin
- **What:** `(MORE)` markers dropped by text match regardless of how the
  indent classified them; a `(CONT'D)` cue (straight or curly apostrophe)
  directly following the same character's dialogue merges into one
  speech. Mid-scene `(CONT'D)` after intervening action keeps its cue
  (authentic convention).
- **Default:** rejoin on.
- **App options:** toggle "Rejoin dialogue split by page breaks";
  possible extra: "strip all (CONT'D) extensions" (not built).
- **Code:** `src/fountain/serialize.ts` (`prepare`, `MORE_PAREN`, `CONTD`).

### 8a. (CONT'D) normalization (option, default auto)
- **What:** page-break (MORE)/(CONT'D) pairs are meaningless in reflow
  (rejoined by #8); mid-scene (CONT'D) marks a speaker continuing
  through action — standard but taste-dependent. `contdMode`: **auto**
  (default) strips source cues' (CONT'D) and re-adds it exactly where
  the rule applies (same speaker continues, reset at scene/transition);
  **strip** removes all; **keep** preserves the source.
- **Stage-1 only (2026-07-30):** this and #8 are the ONLY two knobs
  consumed in `fountain/serialize.ts` rather than in `epub/`. They are
  written INTO the `.fountain`, which is the app's cache boundary, so on
  **Fountain input they cannot apply** — the decision is already frozen
  into the cue text. Rendering a fountain that carries `@MARGO (CONT'D)`
  with `contdMode: strip` keeps the (CONT'D); the same PDF converted with
  `strip` does not. This used to exit 0 and silently change nothing;
  `convertFountain` now returns a warning for the provable case (strip
  requested, `(CONT'D)` cues present), which reaches `--json`, the CLI's
  human output, and the app. #8 gets no runtime warning because
  `serialize.ts` drops `(MORE)` unconditionally, so an unrejoined split is
  indistinguishable from an ordinary same-speaker continuation — a guessed
  warning would be worse than this note. The reader rail groups both under
  "From the PDF" and says they apply on the next conversion.
- **Code:** `src/fountain/serialize.ts` (`lastSpeaker` tracking);
  `src/convert.ts` (`stageOneWarnings`, `CONTD_CUE`).

### 8b. Cue keeps its first dialogue line (always on)
- **WRAPPER DELETED 2026-09-14 — the keep is a CHAIN, and the wrapper was
  the bug.** Device verdict, photo-confirmed: on a real script sent via
  **Send-to-Kindle web** (Amazon server conversion to KFX, Enhanced
  Typesetting) cues stranded at page bottoms with their dialogue overleaf.
  One stranded cue introduced a **one-line** speech, which rules out every
  size-based explanation at once: not `orphans`, not an oversized pushed
  chunk, not a speech too tall to fit. Nothing bound the cue forward.
  The cause was the ELEMENT the bind sat on. The 2026-08-11 repair put
  `break-after: avoid` on the `<div class="keep-together">` wrapper. This
  converter honors `break-after` on elements that carry TEXT — #5a proved
  exactly that, on this exact route, on `h2.scene-heading` — but not on a
  structural div, and it does not propagate a last child's `break-after`
  up to its parent. So `p.character`'s own rule governed a break inside
  the wrapper that can never be taken, and the wrapper→dialogue boundary,
  the only break a reader actually meets, was governed by nothing.
  **The fix is the shape #5a already proved here:** the wrapper is gone and
  the chain sits on the paragraphs — `break-after: avoid` on `p.character`
  AND on `p.parenthetical`. A speech is now a flat run of `<p>` inside
  `.dialogue-block`.
  **The parenthetical link reverses a deliberate refusal, on purpose.**
  #5 is right that every avoid link grows the pushed chunk, and while the
  keep was a wrapper that objection held: the chunk already carried cue and
  speech, and one more link bought a blank-bottom page. Without the wrapper
  the chunk is cue + parenthetical + the `orphans` minimum (#17), about four
  lines — the same bound #5a accepted when it traded the heading wrapper for
  a chain. A test pins the link now; the old test pinning its absence is
  deleted with the reason recorded here.
  **Lesson, for the third time in this entry:** a device verdict about a
  PROPERTY does not transfer across a change in DOM shape. 2026-08-11
  recorded that lesson and then the repair committed the same error in the
  other direction, carrying #5a's `h2` verdict onto a `div`.
  **DEVICE VERDICT 2026-09-20: the chain HOLDS. No stranded cues.**
  Owner-observed on a real 112-page script, rebuilt with the chain and sent
  by the same route that produced the failure (Send-to-Kindle web, Amazon
  server conversion to KFX, Enhanced Typesetting), read on the same device.
  This closes the pending item above: the combination is now confirmed, not
  just its individual links, and it is the same route and grade of evidence
  that condemned the wrapper on 2026-09-14.
  **Evidence grade and what it does NOT cover.** One script, one route,
  owner-observed, same grade as #5a's 2026-07-30 pass. The reader was
  looking for cues stranded at page bottoms and found none. He was not
  asked to check scene headings specifically, and no font size was
  stipulated, so "a long speech at max font size" remains unmeasured even
  though the defect it was meant to catch did not appear. A stranded cue on
  any future script reopens this.
- **SUPERSEDED 2026-09-14 (kept for the trail) — the narrowing STRANDED
  CUES, and why.**
- **NARROWED 2026-08-11 — the keep no longer contains the speech.** The
  wrapper closed AFTER the first dialogue TOKEN, and our serializer writes
  a whole speech as one line, so that token is the entire speech: an
  eleven-line speech sat inside one `break-inside: avoid` block. When it
  would not fit, the renderer moved all of it and left the page ending two
  thirds of the way down. That is precisely the oversized-chunk symptom #5
  warns about, arriving from the keep instead of from the chain. It also
  made `keepSpeechesWhole` (#8c) true in practice however it was set,
  which is why that option looked inert on single-paragraph speeches.
  The wrapper now closes BEFORE the first dialogue token, so it holds the
  cue and any parenthetical and nothing else. What holds the speech to its
  cue instead is the chain, every link already device-settled: `break-after:
  avoid` on `p.character` binds and binds ALONE (#5a), and `orphans` on
  `p.dialogue` keeps print's minimum at the page edge (#17).
  **Parentheticals stay unchained on purpose** — see #5; adding that link
  back would rebuild the same oversized chunk in a new place.
  **Device verdict 2026-08-11: the narrowing STRANDED CUES, and why.**
  First read on device after the change showed dialogue separating from its
  character name. The cause was structural, not a renderer limit: with the
  dialogue moved out of the wrapper, the cue became the wrapper's LAST
  child, so `break-after: avoid` on `p.character` governed a break that no
  longer existed. The break a reader meets is the one between the wrapper
  and the dialogue paragraph outside it, and nothing carried it.
  `.keep-together` now carries `break-after: avoid` itself (both spellings),
  which is the link that was missing. The chunk stays bounded at wrapper +
  the `orphans` minimum, not wrapper + speech, so this does not undo the
  narrowing.
  **Lesson worth more than the fix:** #5a settled that `break-after: avoid`
  BINDS on this device, and that was true and irrelevant. The rule was on
  the right element and the wrong side of a wrapper boundary. A verdict
  about a PROPERTY does not transfer across a change in DOM shape.
  **Still pending:** confirm on device that a long speech now splits across
  the page break with its cue attached, at max font size.
- **SUPERSEDED IN PART — device verdict (2026-07-29, same-day A/B on
  device):** the 2026-07-22 conclusion below was a fact about a FORMAT,
  recorded as a fact about the delivery route. Same script, same
  Screepub EPUB, sideloaded over USB in both formats to the same Kindle
  (fw 5.19.2): the **AZW3 strands cues** exactly as documented; the
  **KFX holds every keep — no stranded cues**. Renderer selection
  follows the FILE FORMAT, not how the file arrived. Sideloaded KFX
  gets Enhanced Typesetting; "sideload = legacy renderer" is true only
  of AZW3/MOBI. (The sideloaded .kfx also indexes — it appeared on the
  shelf after a few seconds.) KFX chain: EPUB → jhowell's KFX Output
  plugin ≥2.x in Calibre (drives Kindle Previewer ≥3.32, then repacks
  KPF→KFX). A raw Previewer .kpf is NOT device-readable — it is a zip
  around a SQLite .kdf; the plugin's repack is mandatory. Not verified
  on pre-KFX hardware (< Paperwhite 3, 2015); AZW3 remains the fallback
  there and wherever the plugin/Previewer toolchain is absent.
- **Historical verdict (2026-07-22, photo-confirmed, AZW3 only):** the
  legacy renderer used for USB-sideloaded AZW3/MOBI honors NO keep
  mechanism: page-break-*:avoid ignored; single-cell table wrappers
  split mid-cell; single-paragraph fusion still orphans the cue line
  (no widow/orphan control); display:inline-block overflows the text
  horizontally off-screen. Do not re-attempt IN AZW3. Books delivered
  via Send-to-Kindle (email/app/web) get Enhanced Typesetting, which
  honors the CSS keeps (and, 2026-07-29: arrive as KFX — observed on
  device in documents/Downloads/Items01/). Dual-dialogue side-by-side
  tables photo-confirmed readable on device in AZW3; not yet
  re-verified in sideloaded KFX.
- **What:** inside each dialogue block, the cue and any parentheticals each
  carry `break-after: avoid`, so a cue never strands at a page bottom with
  its speech overleaf (user-requested 2026-07-22). A chain on the
  paragraphs, not a wrapper: the `keep-together` container form was tried
  from 2026-07-22 and deleted 2026-09-14 (see the top of this entry, and
  #5a, which made the same move for headings first).
- **DEVICE VERDICT 2026-09-15, Apple Books, screenshot-confirmed: the
  forward bind does NOT hold, in EITHER spelling.** The first Apple Books
  evidence this project has ever had, and it is negative. Books paginates
  two columns to a spread, so it is a multicol engine as assumed. But with
  the torture fixture at +3 to +5 font sizes: a `p.character` (WREN) sat
  alone at the foot of the right-hand column with its dialogue overleaf on
  the next page, and an `h2.scene-heading` sat alone at the foot of a
  left-hand column with its action in the next column. Both elements carry
  `page-break-after: avoid` AND `break-after: avoid` AND
  `-webkit-column-break-after: avoid`, in separate rules per the iBooks
  shared-block bug. All three were ignored.
  **What this retires:** the 2026-07-30 claim below that the column spelling
  "extends this keep to Apple Books" was never verified and is now disproven
  for the DIRECTIONAL binds. Whether Books honors the INSIDE spelling on a
  wrapper is still untested, and we no longer have a wrapper to test it with.
  **Severity is not uniform.** The right-column case is a real page turn and
  is the defect #8b exists to prevent. The left-column case is a break
  between two columns the reader can see at once, which is cosmetic. Both
  come from the same ignored rule.
  **The rule stays anyway, and this is the reason:** it is proven inert in
  Books but UNTESTED in the Readium family (Thorium, Kobo's phone and tablet
  apps), which is the other half of what it was aimed at. It re-spells links
  that already exist rather than adding new ones, so it costs no pushed
  chunk on any engine that already honors the modern spelling. If a Readium
  pass also comes back negative, delete it as dead CSS rather than leaving
  it to imply a coverage we do not have.
  **The tension worth naming before anyone tries to fix this.** The KFX
  converter ignores `break-after` on a `<div>` and honors it on a `<p>`;
  Books ignores it on both. The only mechanism known to work in Books is
  `break-inside` on a wrapper — which is precisely the shape that produced
  blank-bottomed pages on Kindle and that #8b deleted on 2026-09-14. The two
  engines want opposite structures, and a wrapper big enough to help Books
  is a wrapper big enough to hurt Kindle. Kindle is the primary target, so
  the cue stranding in Apple Books is ACCEPTED until someone finds a third
  mechanism.
- **The column spelling, and the hole it now leaves (2026-07-30, revised
  2026-09-14):** the wrapper used to carry `-webkit-column-break-inside:
  avoid` in a SEPARATE rule — separate because iBooks drops both spellings
  when they share one declaration block — which extended this keep to
  **Apple Books**, whose WebKit honors only the old spelling, and to the
  **Readium family** (Thorium, Kobo's mobile apps). With the wrapper gone
  that rule lists only #10a's table. **There is no `-webkit-column-break-
  after` anywhere in the stylesheet**, so on those engines the cue chain,
  the heading chain (#5a) and the mini-slug chain (#5b) are all inert.
  That was true before this change too — the wrapper only ever shadowed the
  INSIDE spelling — so it is a standing gap, not a regression, and it is
  the obvious next thing to fix here. Unverified on any of those engines. **Not** a kepub claim: kepub paginates with multicol, so the
  old spelling is a plausible reach, but the evidence on record is that it
  ignores break CSS and wants file splits (device-map §6, t=346874).
- **Code:** `src/epub/html.ts` (`speechBlock`), `src/epub/css.ts`
  (`p.character` and `p.parenthetical`).

### 8c. Whole-speech keep (option, default OFF; 2026-07-30)
- **What:** `keepSpeechesWhole` makes each `.dialogue-block` atomic —
  `page-break-inside: avoid; break-inside: avoid` on the block — so a
  speech is never split by a page turn. Off, dialogue flows and only
  #8b's keep applies.
- **Why off by default:** #8b is the proven keep, and it is the one that
  fixes what actually reads as broken (a cue stranded from its speech).
  Atomic speeches buy "never split" by paying in white space: a speech
  that does not fit the room left on a page gets pushed whole, and the
  gap it leaves behind is real — the same arithmetic that made #5a's
  wrapper a bug. Which side of that trade a reader wants is taste, so it
  is a toggle rather than a new default.
- **`avoid` yields:** no renderer can hold a speech taller than a full
  page, so oversize speeches still break bare wherever the text happens
  to land. The cue keep is untouched by this option and stays on in both
  modes — it remains the degradation layer when the whole-block keep
  cannot be honored.
- **App option:** "Keep each speech on one page" (the reader rail's Page
  group), with the tradeoff and the oversize caveat stated in the
  caption beneath it.
- **Reaches Apple Books as of 2026-07-31:** `.dialogue-block` joins the
  column-spelling shadow rule (#5) when this option is on. It had been
  left out of that rule's hand-written selector list, so the toggle was
  inert in Books — which honors only the old spelling — while the
  always-on cue keep worked. The list is now derived from the keep set,
  so this cannot recur for the next keep.
- **Device verdict 2026-08-04: HOLDS, and no rendering failure.** Sideloaded
  KFX on the Kindle, torture fixture with the option ON, read against the
  0.5.1 engine. Speeches held together and Sam reported nothing wrong. The
  longest speech in the set (the MORE-ANCHOR block, taller than one page)
  rendered without the blank-page push the 2016 Previewer reports warned
  about, which was the specific risk this slot named.
  **What this verdict does NOT cover, stated so nobody reads it as more
  than it is:** the white-space COST was not quantified. The entry's own
  framing is that atomic speeches buy "never split" by paying in gaps, and
  which side of that trade a reader wants is taste. Nobody measured how
  much page height went unused, so "HOLDS" here means the mechanism works
  and looks acceptable, not that the trade was judged and preferred. The
  default stays OFF for that reason. Apple Books remains untested.
  **The trade was judged on 2026-08-06, and OFF won.** The gap the paragraph
  above names is now closed. Both arms went on the device as one A/B, the two
  builds differing in exactly one CSS rule (`.dialogue-block` gaining
  `page-break-inside: avoid`, `break-inside: avoid` and the column spelling),
  and titled distinctly so they could be told apart in the library. Sam read
  both and preferred the OFF arm on overall reading quality. No defect in the
  ON arm: it holds, as the 08-04 pass found, and this is taste rather than a
  failure. Still not a MEASUREMENT: nobody counted unused page height, so the
  cost remains unquantified and the preference is a reader's judgment on one
  device at one font size. That is enough to keep the default where it is,
  and not enough to call the option a mistake.
  A first pass at this was invalidated and redone: the build carried the
  split-speech defect (fixed in 0.5.1), which put hard breaks mid-sentence
  and would have been read as a keep failure. Rebuild the fixture against
  current code before any device pass.
  Note the interaction with
  #10b: with this option ON, a tall dual exchange that has fallen back
  to sequential becomes two atomic blocks. Include the longest speech in
  the set at the largest font size: a kept block taller than one page
  historically made Kindle Previewer fail to render pages (jhowell,
  2016), and blank-page pushes are the failure the Blitz framework
  disables Kindle keeps to avoid. The verdict lands here.
- **MOBI:** that dialect ships no stylesheet at all, so no CSS keep — this
  one or any other in this document — applies on the MOBI route. What it
  does now have is the one MARKUP primitive: `<mbp:pagebreak/>`, wired to
  `scenePageBreaks` on 2026-07-30 (#1). Keeps stay EPUB-only; forced
  breaks no longer are.
- **Code:** `src/options.ts` (`keepSpeechesWhole`), `src/epub/css.ts`
  (`speechKeep` → `.dialogue-block`),
  `app/Sources/ScreepubKit/FormatSettings.swift`.

### 9. Page furniture stripping
- **What:** page numbers (bare/dashed/labeled), shooting-script scene
  numbers, revision slugs ("Blue Rev. (6/12/26)"), draft stamps, header
  dates; plus recurrence-based watermark suppression (text on ≥ max(3,
  40% of pages) distinct pages).
- **Default:** all stripping on; recurrence threshold 40%/3 pages.
- **App options:** master toggle unlikely to be wanted; expose the
  watermark recurrence threshold as advanced only.
- **Code:** `src/parser/classify.ts`, `src/parser/boilerplate.ts`.

### 10. Final Draft double-print dedup
- **What:** FD double-prints (MORE)/(CONT'D) furniture at identical
  coordinates with a zero-width item between copies → text doubled
  ("RYAN (CONT'D)RYAN (CONT'D)") and merges failed. Exact-overlap
  duplicates deduped at extraction.
- **Default:** always on; not an option (pure artifact fix). Log only.
- **Code:** `src/parser/extract.ts` (dup check in item join).

### 9a. Dual-margin scene numbers collapsed
- **What:** shooting scripts print the scene number in BOTH margins of
  the slugline row; joined they leaked as "2  2" action lines and never
  attached (found in testing). Duplicated tokens collapse to one, which
  then classifies and attaches as the scene number.
- **Inline variant (2026-07-22, found in another test script):** some
  generators put the numbers on the heading row itself ("2 EXT. WOODS -
  DAY 2"), which defeated the ^INT/^EXT anchor — 0 scenes detected.
  Classifier now strips a same-token leading/trailing number pair and
  attaches it as `sceneNumber` when the inner text is a slugline.
- **Code:** `src/parser/extract.ts` (normal-line emit),
  `src/parser/classify.ts` (`DUAL_MARGIN_HEADING`).

### 9b. Hybrid character cues
- **What:** MARGO/DEV (shared), COP #2, MOM & DAD — the name pattern
  now allows / & # and digits; previously such cues fell to action and
  their speeches collapsed (a lyrics block found in testing).
- **Code:** `src/parser/classify.ts` (CHARACTER_NAME).

### 9c. Width-aware item joining
- **What:** gap detection now uses pdf.js's real item widths (falling
  back to the len×6 estimate) — fixes phantom spaces in split names
  ("Jo hn Sm ith") and sharpens dual-dialogue boundaries.
- **Code:** `src/parser/extract.ts` (`endX`).

### 9d. Inline bold/italic/underline pass-through
- **What:** per-item font styles detected from PostScript names
  ("CourierPrime-Italic" etc.; getOperatorList forces font resolution —
  getTextContent alone doesn't load fonts). Styled runs travel as
  fountain emphasis (*i*, **b**, ***bi***) in a `styledText` variant
  carried beside plain text; classification always uses plain, and only
  dialogue/action emit styled (markers would break cue/parenthetical/
  slug recognition). Rendered as em/strong (EPUB) and i/b (MOBI).
  Punctuation-only styled items never wrap alone (no "*,*").
- **Underline (2026-08-04):** PDFs DRAW underline as vector art rather than
  selecting a font, so it is invisible to the font-name check above. A single
  walk of each page's operator list collects drawn rules — pdf.js folds all
  painting into `constructPath`, whose `minMax` bbox alone separates a rule
  from a box — and an item is underlined when a mark sits 0.5pt above to
  3.5pt below its baseline and covers ≥ 60% of its width. Marks must be
  ≤ 2.5pt tall, ≥ 4pt wide, and < 85% of the page (header rules). The CTM is
  tracked through save/restore/transform: Final Draft draws under a y-flip
  ([1,0,0,-1,0,H]), so both bbox corners are transformed and min/maxed, and
  only genuinely skewed matrices are skipped. Every paint op counts except
  `endPath` (`W n` clip paths, which paint nothing) — real underlines are
  STROKED with a zero-height bbox, not filled. Marks nest with the underscore
  innermost (`**_x_**`), which is the order both renderers already unwrap.
- **Measured bound:** detection is per text ITEM, so a rule under only PART of
  a pdf.js run is dropped rather than applied to the whole run. Two of
  Highland's four underlines in the local set are that shape (a 36pt rule
  under a 129pt item). The trade is deliberate: a false positive puts
  underscores around text that is not underlined in a shipped book, a false
  negative just preserves the pre-2026-08-04 behavior. Fixing it means
  splitting an item at character offsets, which needs per-glyph advances.
- **Decoys (torture fixture, page 6):** a strikethrough, a table-cell border
  below the text band, and a page-wide rule sit beside the real underline,
  each rejected by a different filter, each asserted silent.
- **Device verdict 2026-08-06: RENDERS, and the decoys stay clean.**
  Sideloaded KFX on the Kindle, the torture fixture's sheet 6. The underlined
  word rendered underlined; the struck phrase, the table border and the
  page-wide rule all rendered with no underline, which is the half that would
  have shown a loosened threshold. Only detection was at issue here, since
  `span.underline` already shipped, so this closes the entry rather than
  opening a rendering question. Apple Books untested.
- **Code:** `src/parser/extract.ts` (`stampFontStyles`, `stampUnderlines`,
  `collectUnderlineMarks`, `markUnderlinesItem`, `joinLine`),
  `src/epub/html.ts` + `src/mobi/html.ts` (`inlineEmphasis`/`inline`).

### 9e. Cue extensions tolerate a missing closing period
- **What:** `(O.S)`, `(V.O)`, `(O.C)` — the period-less spellings writers
  routinely type — now count as dialogue extensions. Previously only the
  fully punctuated `(O.S.)` matched, so the bare form skipped the
  extension branch and then hit the "periods only in ellipsis" guard:
  the cue fell to action and its speech collapsed with it (same failure
  shape as 9b). A script can spell one speaker BOTH ways — THE LAST
  VIDEO STORE has "RADIO VOICE (O.S)" on p7 and "RADIO VOICE (O.S.)" on
  p8, which
  rendered as description and cue respectively. Once both are cues, 8a
  correctly adds `(CONT'D)` to the second.
- **Not changed:** `(CONT'D)`/`(CONT.)` keep requiring their existing
  spellings — no reported miss, and loosening them buys nothing.
- **2026-07-30 — dotted names pass the period guard.** "Periods only in
  ellipsis" rejected every bare dotted cue (`MR. SMITH`, `J.J.`,
  `ANNA B.`, `E.B. WHITE`) while the extension branch admitted
  `MR. SMITH (V.O.)` — recognition depended on incidental punctuation,
  the same asymmetry as 9e's own bug. The guard now accepts periods in
  abbreviation position only: mid-name a period may cap a 1-4 letter run
  (`MR.`, `CAPT.`, `E.B.`), and at the end of the name only
  single-letter initials qualify (`ANNA B.`, `J.J.`). A period closing a
  longer final word (`HE STOPS.`, `STOP.`) is still sentence punctuation
  and still vetoes — the terminal rule protects the dialogue/cue band
  overlap, where a shouted `STOP.` becoming a phantom speaker would
  swallow the next line as its speech. `CHARACTER_NAME` admits `.` in
  its body to match. Verified: character-element counts across all five
  generator fixtures are byte-identical (none contains a dotted cue),
  so the corpus is untouched; the new shapes are pinned by unit tests.
- **Code:** `src/parser/classify.ts` (`DIALOGUE_EXTENSIONS`).

### 9f. Cue rescue from the character roster
- **What:** classification is geometric, but the text guards in
  `isLikelyCharacterName` can overrule position — a dropped period (9e), a
  curly apostrophe, a lowercase cue. Each rejection costs TWO elements:
  with no active character the speech underneath falls to action too (9
  paragraphs lost in one 94-page script). A second pass now promotes an
  `action` element back to `character` when ALL hold: cue-band indent, the
  normalized text matches a name already established elsewhere in the same
  script, and a dialogue-indent speech follows. It cannot invent
  characters — the roster is corroboration — and it is guard-agnostic, so
  it catches variants nobody has reported yet.
- **Ordering:** runs after boilerplate suppression (so a watermark can't
  be rescued into permanent immunity) and before character extraction (so
  rescued speeches reach the dialogue counts). Title-page elements are
  never rescued.
- **Also:** `CHARACTER_NAME` now accepts curly apostrophes (`MARGO’S MOM`),
  matching how §8 already treats `(CONT'D)`.
- **Code:** `src/parser/rescue.ts`, `src/parser/index.ts`,
  `src/parser/classify.ts` (`normalizeCueName`, `CHARACTER_NAME`).

### 10a. Dual dialogue — de-interleaved to sequential speeches
- **What:** simultaneous two-column speeches (seen in two different test
  scripts, one with 23 lines of dual dialogue) previously interleaved
  into garbage because extraction joins by Y line. Now: a dual-cue line
  (two cue-shaped clusters) anchors a region; the column boundary starts
  at rightCueX−13% and refines only
  LEFTWARD to the right column's text edge (parentheticals sit deeper and
  must not drag it); body lines partition by start-x; regions end on a
  full-width (straddling) line, a cue-shaped left-only line, text left of
  the learned column edge, or a new dual-cue pair. Columns emit as left
  speech then right speech — the standard reflowable treatment.
- **Default:** de-interleaving always on (correctness); rendering mode
  `dualDialogue: 'sideBySide'` (default) | `'sequential'`. Side-by-side:
  the right cue serializes with Fountain's `^`, and the renderer emits a
  full-width two-cell `table.dual-dialogue` (50/50, top-aligned,
  page-break-inside avoid) — tables are the one column construct
  Kindle's renderer honors — **unless the exchange is tall, in which case
  the EPUB falls back to sequential speeches (see #10b)**. Wider than the
  dialogue column by design. MOBI gets a plain width-50% table, at any
  height.
- **App option:** the reader rail's Dialogue group → "Dual dialogue". Known
  limitation: a short action line immediately after a dual block with no
  intervening cue can absorb into the left speech.
- **Merge bug, fixed 2026-09-23:** before that date, once a base held
  `dualDialogue: 'sequential'`, nothing downstream, including the window's
  toggle back to side by side, could move it: an explicit `'sideBySide'`
  was read the same as absent and fell through to the base. The merge now
  accepts either value explicitly.
- **Code:** `src/parser/extract.ts` (`deinterleaveDualDialogue`).

### 10b. Tall dual exchanges degrade to sequential (2026-07-30)
- **What:** the EPUB renderer measures both columns before it emits: if
  the taller one exceeds **12 estimated rendered lines**, the exchange is
  emitted as two ordinary dialogue blocks — left speech, then right —
  each carrying its own #8b cue keep. Short exchanges, the common case,
  still get the table. The estimate counts each cell's tag-stripped text
  (trailing newline included) at ~30 characters per half-width line, with
  a floor of one line per cell, so a one-word cue still costs a line.
- **Why:** the table of #10a has no good behavior at height. A
  keep-honoring renderer is expected to treat it as one object and push
  it whole, making a tall exchange a page-sized chunk that wastes a page
  bottom; the legacy AZW3/MOBI renderer splits it mid-cell instead, which
  #8b already records as its own kind of broken.
- **The push claim is inference, not measurement:** it is extrapolated
  from #5a's observed half-empty pages (an unbreakable chunk pushed
  whole), not from a dual table measured on device. #8b confirms only
  that the side-by-side table is *readable* in AZW3.
- **Why 12:** roughly half a typical device page. Past that, the odds the
  table has to push — wasting up to its own height at the bottom of a
  page — climb faster than the side-by-side reading is worth.
- **What it costs:** the fallback renders exactly what
  `dualDialogue: 'sequential'` already produces, and it inherits that
  mode's known property — the simultaneity cue is lost; the two speeches
  simply read in order.
- **Interaction with #8c:** the bounded-chunk rationale above holds fully
  only with `keepSpeechesWhole` OFF. With it ON, the two blocks the
  fallback emits are each atomic, so a 14-line exchange trades an
  unbreakable 14-line table for an unbreakable 14-line block — the
  simultaneity is still lost, but the chunk is not bounded. The
  threshold does not know about the option.
- **Honest scope:** this is a markup-level decision taken in the EPUB
  renderer, not a stylesheet knob and not an option. It is EPUB-only —
  the MOBI path still emits its table at any height.
- **Device verdict 2026-08-03: HOLDS — the fallback fires.** Sideloaded
  KFX on the Kindle, torture fixture sheet 7, which carries the two cases
  back to back: a short exchange rendered side by side as a table, and one
  whose taller column passes 12 estimated lines rendered as two ordinary
  sequential speeches. Both behaved as specified.
  **Still open:** whether 12 is the RIGHT cut. This pass proves the
  threshold fires, not that it fires in the right place, since only one
  exchange either side of it was tried. Apple Books untested.
- **Code:** `src/epub/html.ts` (`DUAL_SEQUENTIAL_LINE_THRESHOLD`,
  `EST_CHARS_PER_DUAL_LINE`, `dual_dialogue_end`).

## Metadata & navigation

### 11. Generated title page
- **What:** standalone title page from the detected title block
  (title-cased if ALL-CAPS) + "Written by" credit. Contact info, WGA
  numbers, copyright, draft stamps on the PDF's title page are excluded.
  `--title` / `--author` CLI overrides exist.
- **App options:** include/exclude title page; editable title/author
  fields (already CLI-exposed).
- **MOBI arm (2026-07-31):** the MOBI dialect emits its own centered
  title block followed by `<mbp:pagebreak/>`, and now honors the same
  toggle. It had ignored it since the dialect was written: OFF dropped
  the EPUB's title file, manifest item, spine itemref and nav landmark
  and left the `.mobi` opening on a title page anyway. The knob only
  became reachable there when `tokensToMobiHtml` started taking
  FormatOptions for #1's MOBI arm, and was fixed in the same pass that
  found it. With the title page off there is no leading break either, so
  the body does not open on a blank page.
- **Code:** `src/fountain/serialize.ts` (`extractTitleMeta`),
  `src/epub/build.ts` (`titlePageXhtml`), `src/mobi/html.ts`.

### 12. Scene-level TOC
- **What:** every PRIMARY slugline becomes a TOC entry (nav.xhtml) linking
  to its anchor; landmarks for title page / begin-reading. Mini-slugs are
  deliberately absent — "LATER" is not a destination (#5b).
- **Default:** on, labeled "Scenes".
- **App options:** TOC on/off; possible "TOC every N scenes" for
   150-scene scripts where the drawer gets long.
- **Code:** `src/epub/html.ts` (toc), `src/epub/build.ts` (`navXhtml`).

### 13. Shooting-script scene numbers — kept in Fountain, hidden in EPUB
- **What:** scene numbers ("1A.", "2.2.") attach to headings and
  serialize as Fountain `#1A.#`, but the EPUB heading renders without
  them (fountain-js splits them off; the HTML layer doesn't re-add).
- **Default:** hidden in EPUB.
- **App option:** "Show scene numbers in headings" — small change in
  `src/epub/html.ts` (`scene_heading` case, `t.scene_number`).

### 13a. Original page-number markers (option, default ON; 2026-09-21)
- **What:** `showPageMarkers` emits the PDF's printed pagination as small
  right-flush dimmed markers ("47.") at page boundaries — page count is how
  scripts are evaluated (1 page ≈ 1 minute), and reflow otherwise erases
  it. Numbering anchors to the stripped printed page-number furniture
  (mode of printed−sheet offsets, so the title page never counts and
  margin scene numbers can't pollute). Markers travel as fountain
  synopsis lines (`= pg N` — invisible to other fountain tools) and land
  only at block boundaries: a page turning mid-speech defers its marker
  to the next block. PDF input only (fountain has no pages).
- **Rendering:** the marker rides inside the next block as a floated
  `<span>`, so it costs no line (it used to be its own `<p>` with a 1em
  top margin). In EPUB the same span is the EPUB3 pagination anchor
  (`epub:type="pagebreak"` + `title`), and a hidden `page-list` nav gives
  capable readers real page numbers and page-jump. MOBI gets the floated
  span only. If a legacy renderer ignores `float`, the number degrades to
  inline text at the head of the paragraph.
- **Dimming, not gray (2026-07-30):** the marker now recedes via
  `opacity: 0.6` instead of a hardcoded gray. A fixed gray was picked
  against a white page and fought every themed background; opacity is
  relative to the theme's own text color, so the marker tracks dark and
  sepia as well as white. Engines with no opacity support render it at
  full strength — a harmless degrade — and Enhanced Typesetting lists
  opacity as supported (Guidelines 2026.2 §18.1).
- **Default flipped ON (2026-09-21):** it shipped off. The argument for off
  was that the markers are furniture and a clean book should not carry the
  source's page breaks; the argument that won is that page numbers are how
  a script is *discussed*. Notes come back against printed pages ("the beat
  on 42 is late"), and a book that dropped the pagination made every one of
  those notes unfindable, with nothing else in it carrying the printed
  numbering. Anyone who wants the clean book still has the knob.
  **`showSceneNumbers` was deliberately NOT flipped with it** (see 13): a
  scene number is an intentional property of a draft, present or absent
  because a writer decided, so adding one is inventing content. A page
  number is navigation, and the PDF already had it.
  This is a stage-2 knob, so it is written into the `.fountain` when the PDF
  is read: existing scripts keep whatever they were converted with, and only
  a fresh conversion picks the new default up.
- **Code:** `src/fountain/serialize.ts` (`printedPageOffset`),
  `src/epub/html.ts` + `src/mobi/html.ts` (synopsis case),
  `src/epub/css.ts` (`span.page-marker`), `src/epub/build.ts` (page-list).

## Guards (behavior, not formatting — app should surface as dialogs)

### 14. Scanned-PDF bail-out
- <3 text lines/page average → error "run OCR first", no silent garbage.
  `src/convert.ts` (`MIN_LINES_PER_PAGE`).

### 15. Not-a-screenplay guard
- 0 scenes **and** 0 dialogue → error unless `--force` (caught a pitch
  deck in testing). 0 scenes with dialogue → warning only.
  `src/convert.ts`.

## Mac app notes

**These eighteen options now have two interfaces, not one.** Besides the
SwiftUI reader rail described below, they are the **Tune surface of the
cross-platform Tauri window** (`desktop/ui/tune.js`), in the same four groups
— the page, dialogue, the text, what the book carries — plus the four
`Read from the PDF` knobs. The window owns no formatting rule of its own: it
reaches every option through the CLI, `screepub settings <fountain> --set`
to read and store the per-script sidecar and `--options-json` to re-render
from the cached `.fountain`, so the engine remains the only thing that
decides what a knob means. The `From the PDF` knobs carry the same "won't
change the preview" caveat there as in the Swift rail: they are decided while
the PDF is being read and only the next conversion from the PDF can apply
them.

**The two interfaces disagree about how many those are, and the window is
the one that is right.** `ReaderRail.swift`'s `From the PDF` section holds
two knobs (`rejoinSplitDialogue`, `contdMode`) and says in a comment that
they are "the only knobs consumed in fountain/serialize.ts". They are not:
`showPageMarkers` and `dualDialogue` are read there too (serialize.ts's
`= pg N` marker and its `^` caret), so both are written INTO the `.fountain`
when the PDF is read and neither can move a preview that re-renders from
that cached `.fountain`. The Swift rail files them under `Content` and
`Dialogue`, where they read as live knobs. `desktop/ui/tune.js` groups all
four together under `Read from the PDF` with the caveat. Whoever next opens
the Mac app should move those two; nothing in the engine needs to change.

- All knobs above funnel into two seams: **CSS generation**
  (`src/epub/css.ts` — make it a function of an options object) and
  **serializer/packing flags** (`tokensToBody` opts, `prepare` in the
  serializer). The parser layer should stay option-free.
- The `.fountain` intermediate is the natural preview/re-render boundary:
  app can cache stage 1 and re-run stages 2–3 live as options change.
- Validation habit worth keeping in the app: `epubcheck` after every
  render config change (all tweaks above shipped at 0 errors/0 warnings
  across the five-generator fixture set + additional real-world scripts).
- **Device routes (2026-07-22, KFX rung added 2026-07-29, not formatting
  knobs but adjacent):**
  Kindle → KFX to documents/ when the full toolchain is present (Calibre
  + KFX plugin + Kindle Previewer, §8b), AZW3 with Calibre alone, MOBI
  with neither — one ladder shared by the result page, the reader rail,
  and the save flow; Kobo → EPUB to volume root, or
  KEPUB via Calibre when the "as KEPUB" checkbox on the result page's
  send block is on — shown only while Kobo is the chosen destination
  (default OFF — Kobo's kepub renderer mis-justifies em-dashes/
  ellipses, endemic in dialogue; kit-check verifies koboSpan markup);
  tolino → EPUB into root `Books/`; reMarkable → original PDF over the
  USB web interface (its EPUB renderer re-typesets, so reflow adds
  nothing on a 7.8–11.8" screen). Detection: `.kobo` dir, tolino
  volume name, Kindle documents/, reMarkable HTTP probe.
- **Script preview reader (2026-07-22):** READ SCRIPT on the result page
  opens a window rendering the engine's real preview HTML
  (`--preview-html`; the same markup as the EPUB with the CSS inlined).
  The rail edits a per-script sidecar (`<Stem>.screepub.json`,
  ScriptSettings.swift, lenient per-field decode) with debounced,
  serialized re-renders from the cached .fountain; "Save as app
  defaults" promotes sidecar values to the global keys
  (AppSettings.setFormatSettings). Every re-render rewrites the
  library EPUB AND MOBI (atomic temp+rename, engine-side) so sends
  match the preview; main-window conversions also load the sidecar, so
  re-dropping a tuned script keeps its tuning (note: a NEW script whose
  filename stem matches an old one inherits that sidecar — treated as
  same-script-new-draft).
- **One formatting surface (2026-07-30):** the Settings → Formatting tab
  is gone. It carried all 15 knobs beside a hand-drawn schematic
  (`LayoutPreview`, deleted with it) while the reader rail carried only 11
  beside the real engine output — so the window that could show you a
  change was the one that could not make four of them
  (`keepSceneHeadingWithScene`, `includeTitlePage`, `rejoinSplitDialogue`,
  `contdMode`). The rail now owns all 18 (15 at the move, plus #8c's
  `keepSpeechesWhole`, #17's `printSplitMinimums` and #18's
  `preserveFontShifts`), grouped Page / Dialogue / Text / Content / From
  the PDF. Settings keeps General (library, updates) and Devices (default
  preset, KFX toolchain, tolino/reMarkable notes). **Adding a knob means
  adding one control to `ReaderRail.swift`** — there is deliberately no
  second surface to keep in sync. Settings sets the coarse default via a
  preset; per-script tuning belongs beside a live render.
- **Device presets (2026-07-22):** `DevicePreset` (ScreepubKit) bundles
  a full FormatSettings per device class — "Kindle e-ink (6\")" is the
  baseline (== defaults); "Phone / narrow screen" flips dual dialogue to
  sequential (side-by-side halves are an unreadable sliver on a narrow
  screen) and widens the dialogue column (10% side margins). Applying a
  preset replaces the whole FormatSettings — globally via Settings →
  Devices "Load device preset", or per-script via the reader rail's own
  "Load device preset" (then persisted to the sidecar). Responsive
  reflow is impossible in a fixed e-book, so a conversion-time preset is
  the mechanism. Adding a preset: extend the `DevicePreset` enum only —
  the two menus and kit-check iterate `allCases`.

### 18. Block font shifts (option, default ON; 2026-08-04)
- **What:** `preserveFontShifts` renders block-level font family and size
  shifts the PDF carried — text-message inserts, chyrons, letters, on-screen
  title cards. The parser computes the document's dominant (family bucket,
  size) weighted by CHARACTER COUNT, and stamps a line with `fmt` when ≥ 80%
  of its resolved characters agree on one deviation. Blocks take the shift
  only when every line agrees, and an fmt change breaks a block, so a chyron
  glued to plain action isolates instead of vanishing.
- **Vocabulary:** family ∈ `mono|serif|sans|cursive`, size ∈ `-1|+1|+2`.
  Serialized as Fountain notes: `[[fmt: sans]]`, `[[fmt: +2]]`,
  `[[fmt: sans +2]]` — on its own line glued above an action block, inline
  ahead of a dialogue line. The notes channel is lossless through fountain-js
  and invisible by spec, so a tool that never heard of Screepub ignores it.
- **Thresholds:** size step from the ratio to the dominant — ≤ 0.85 is `-1`,
  1.15–1.4 is `+1`, > 1.4 is `+2` — each additionally requiring ≥ 1.5pt of
  absolute change, so float jitter in a text matrix can never mint a step.
  Size is the text matrix's vertical scale (`transform[3]`, absolute).
  Family buckets on the PostScript base name with its subset prefix stripped,
  lowercased, non-alphanumerics removed: `courier|mono|lettergothic|prestige`
  → mono, `times|georgia|garamond|palatino|caslon|baskerville|minion` →
  serif, `script|hand|brush|comic` → cursive, everything else sans. Bare
  "gothic" is excluded (Century Gothic is a sans) and so is bare "roman"
  ("-Roman" is the REGULAR weight of Helvetica-Roman and AvenirNext-Roman;
  Times matches on "times" anyway). Weight and slope tokens match nothing
  here: style is #9d's business.
- **Always serialized, knob gates RENDERING:** the `.fountain` carries the
  notes whatever the setting, because it is the app's cache boundary and must
  be byte-stable under a settings flip — flipping this re-renders without
  re-parsing the PDF. That is also why its rail control sits in "Text" with
  the other render-time knobs, not in "From the PDF".
- **Note stripping (general correction):** both renderers now strip ALL
  `[[...]]` notes from rendered text. Before this entry we emitted none and
  stripped none, so a hand-written note rendered as literal text, which the
  Fountain spec forbids. One copy in `src/fountain/notes.ts`, imported by
  both renderers (the #5b precedent).
- **Rendering:** EPUB adds `fmt-mono|fmt-serif|fmt-sans|fmt-cursive` and
  `fmt-minus1|fmt-plus1|fmt-plus2` to the block's paragraphs; sizes are
  0.85/1.2/1.5em (vertical-in-em invariant, no line-height, no max-width, no
  CSS3 value functions). A family class sits on the paragraph, so it locally
  overrides the body `fontFamily` option (#6). MOBI renders the size arm as
  `<font size="-1|+1|+2">` and DROPS family: MOBI 6 face support is not
  reliable enough to spend the markup on.
- **Bounds:** block-level only — inline (mid-line) family/size runs are an
  explicit non-goal. Only action and dialogue carry notes, so a shifted line
  that classifies as a cue, parenthetical, transition or MINI-SLUG loses its
  shift: markers break slug recognition (#5b, #9d). That is not theoretical —
  the fixture's `+1` case first read "INSERT: THE INDEX CARD", which is a
  secondary-slug shape, and tested nothing. Dual-dialogue lines carry no fmt
  either: the column-partition pass (#10a) joins its columns to strings
  without retaining the per-column items, and a simultaneous exchange in a
  deviant face is rare enough not to justify restructuring it. Embedding the
  PDF's actual fonts is also out of scope; the classes name generic stacks.
- **Uniform scripts are untouched:** a single-face, single-size screenplay
  produces zero fmt anywhere by construction, so its `.fountain` and both its
  outputs are byte-identical to before this entry.
- **Measured on the real corpus (2026-08-04), and the honest result:** the
  `.fountain` of every script in the local set is byte-identical before and
  after this entry — **zero** notes emitted across all five generators. Final
  Draft, Highland, Celtx and Chromium are single-face, single-size and
  produce no shifted lines at all. Fade In produces 7 shifted lines out of
  3266 (five `sans` from an AvenirNext run, two `sans +2` from a 36pt display
  face) and still emits no note, because **all seven are on its TITLE page**,
  which never reaches body serialization. So this entry is exercised only by
  the invented fixture today. That is a statement about this corpus, not
  about the feature: chyrons, inserts and on-screen text are ordinary
  screenplay furniture, and none of these five happen to carry one in the
  body. Worth re-measuring when a script that does joins the set.
- **App option:** "Keep the PDF's font shifts" (reader rail, Text group).
- **Device verdict 2026-08-06: RENDERS, and the family survives the
  publisher-font toggle in BOTH states.** Sideloaded KFX on the Kindle, the
  torture fixture's sheet 6, read against merged main. All five shifted
  blocks rendered distinctly from the body: the Helvetica chyron and the
  text-message dialogue line in a visibly different face, and the three size
  steps visibly smaller, larger and largest. No keep broke around the `+2`
  block, which was the specific risk this slot named.
  The publisher-font result is the surprise, and the better one. The slot
  expected the family to survive only with the reader's publisher-font option
  ON; it held with the option OFF too. KFX metadata carries
  `override_kindle_font=False`, which is the likely reason. Treat that as the
  mechanism, not as proven causation: one device, one firmware.
  **What this verdict does NOT cover:** Apple Books is untested, so #6b's
  `ibooks:specified-fonts` meta is still unexercised for this entry. And the
  fixture remains the only thing that exercises #18 at all, because no real
  script in the local set carries a body font shift. A generator that sets
  its inserts differently could still bucket in a way this fixture cannot
  predict.
- **Code:** `src/parser/extract.ts` (`familyBucket`, `fontRuns`,
  `stampLineFmt`), `src/parser/group.ts`, `src/parser/classify.ts`,
  `src/fountain/serialize.ts` (`fmtNote`), `src/fountain/notes.ts`,
  `src/epub/html.ts` + `src/epub/css.ts`, `src/mobi/html.ts`,
  `src/options.ts`, app FormatSettings + ReaderRail.

### 19. Pictures recognised, recorded as notes (always on, no option; 2026-10-01)
- **What:** a PDF conversion finds the pictures a script paints (storyboard
  panels, reference stills, a lookbook plate, a designed title card), keeps
  the ones that are art, and records each kept one in the `.fountain` as an
  invisible Fountain note, `[[img: p012-01.png w=71 full]]`, with its pixels
  as a PNG in a `<stem>.images/` folder beside it.
- **The book does not change in this phase (phase 1 of images in the
  book).** Both renderers drop the note
  unread: a standalone note is a `note` token, which neither renderer's switch
  emits, and a note folded into a line goes through `stripNotes` (#18). So the
  EPUB and the MOBI are what they were before this entry, and the reader never
  sees a picture yet. Showing them is phase 2 (the `includeImages` option and
  EPUB figures), phase 3 (MOBI image records) and phase 4 (cover promotion):
  `docs/superpowers/specs/2026-10-01-images-in-the-book-design.md`.
- **Where a picture comes from, and where it lands.** Extraction runs the
  image pass on the operator list the text pass already fetched, one page at a
  time, and reads each picture's pixels while the page is live (pdf.js drops
  them once the page is released). Pixels are flattened onto WHITE, since the
  PNG is colour type 2 with no alpha and a transparent pixel would otherwise
  take whatever colour was stored under it, frequently black; downscaled so
  the longer edge is at most `IMAGE_MAX_EDGE = 1400` px (clear of every e-ink
  panel Screepub ships to: a Paperwhite is 1236x1648); and encoded at parse
  time. Kept pictures join the element stream where they sat on the page:
  page ascending, then top edge descending, a text line first on a tie
  (`insertImages`, step 5.8 of `parseLines`). That step runs after
  `mergeSplitSpeeches`, so a plate at the top of a page cannot split a speech
  the page break split, and before the metadata passes, so scene indices count
  it.
- **The note:** `[[img: <file> w=<N>]]`, with ` full` before the closing
  brackets when the picture covers at least `FULL_PAGE_AREA_FRAC = 0.70` of
  its sheet. `<file>` is `pNNN-NN.png`, the page number and the picture's
  order on that page, so a re-conversion reproduces byte for byte. `w=` is an
  INTEGER percentage of the script's TEXT COLUMN, not of the page (6.0 of an
  8.5 inch sheet, 70.6%), clamped to 5..100: the e-book has no print margins,
  so measuring against the page would set every picture about 30% too
  narrow. Always serialized, like #18's notes, because the `.fountain` is the
  cache boundary and has to stay byte-stable under a later settings flip. One
  reader, `parseImgNote` in `src/fountain/notes.ts` (the #5b/#18 rule: one
  module, imported by everything that reads the note). It is case-sensitive on
  `img:`, a width it cannot read means 100 (too large beats too small to see),
  and the filename must pass the allowlist.
- **The allowlist is a security boundary, not tidiness:**
  `^[A-Za-z0-9._-]{1,64}\.png$`. A `.fountain` can be hand-edited or arrive
  from someone else, and this string becomes a zip entry path and a filename
  read off disk. So the class holds no separator, no `%`, nothing absolute,
  and only a lowercase `.png`, and a name that fails is dropped, never
  repaired. The end anchor is pinned by notes that only it refuses
  (`a.png/../../x`, `a.png.exe`). The picture folder's reader applies the same
  `isSafeImageFile` rather than a second check. Its PRUNE uses a narrower test
  (`isGeneratedImageFile`, only names the engine writes), so a user's own
  `board-01.png` in that folder is never deleted.
- **The folder:** `<stem>.images/` beside the `.fountain` (in the script's
  library folder under `--library`), written by the CLI through
  `src/library-images.ts`, pictures first so the `.fountain` never names a
  file that is not there. A run removes the generated pictures it did not
  produce, and a run with no pictures clears the last one's folder and creates
  none. `--no-fountain` writes neither. Reading the folder back refuses
  symlinks, directories and FIFOs, any file over `MAX_IMAGE_FILE_BYTES`
  (derived from `IMAGE_MAX_EDGE`, so it is what this engine's encoder could
  have written), and anything that is not a PNG. Only the folder itself is
  ever created, never a missing parent. A folder that cannot be written is the
  `unwritable` error, naming the folder and what was already written, and
  saying the `.fountain` was not updated: on a re-conversion the previous one
  stays, still naming the previous pictures. The `--json` answer gains `imagesPath` (where a folder was
  written) and `imageCount` (how many pictures the conversion carried, which
  `--no-fountain` reports too); both are omitted when there is nothing to say.
- **What counts as a picture: six rules, in this order, and the first to fire
  is the one reported** (`keepArtwork` in `src/parser/artwork.ts`; its
  `droppedBy` histogram is the retune instrument, counted in PLACEMENTS):
  1. **Pixels:** `MIN_PIXELS = 32` source pixels on each edge, as the paint
     operator declared them. A 16x16 logo glyph is not worth carrying however
     large the page draws it.
  2. **Width:** `MIN_WIDTH_FRAC = 0.10` of page width, for a tall narrow strip
     (a sidebar rule, a revision bar) that the area floor cannot see.
  3. **Height:** `MIN_HEIGHT_FRAC = 0.03` of page height. It can barely fire
     while the area floor is also 3%: a box under 3% of page height reaches 3%
     of the area only by being wider than the page.
  4. **Area:** `MIN_AREA_FRAC = 0.03` of the page (about 1.9 by 1.45 inches on
     US Letter), the product of the two linear fractions. Aimed at decoration,
     not at logos: a letterhead logo clears it, which is what rule 5 is for.
     Do not raise it to catch one.
  5. **Furniture:** the identical picture on `FURNITURE_PAGES = 3` or more
     distinct sheets, OR on at least `FURNITURE_SHARE_MIN_PAGES = 2` sheets
     that are also `FURNITURE_FRAC = 0.25` of the document. Sameness is a hash
     of the ENCODED PNG, because one logo is often a different pdf.js object on
     every sheet. The sibling of `suppressBoilerplate` (#9), set to catch
     earlier: a false positive there hides a line of script, here it loses a
     picture. `FURNITURE_SHARE_MIN_PAGES` is derived rather than chosen:
     without it a lone picture in a four-sheet document is 25% "repeated".
  6. **Background:** covering `BACKGROUND_AREA_FRAC = 0.90` of a page that
     also carries `BACKGROUND_TEXT_LINES = 20` or more text lines (about a
     third of a 55-line page). Neither half means anything alone: the text
     count is what tells a watermark printed behind the script from a designed
     title card of the same geometry.
  Every comparison is written `!(value >= floor)`, so a NaN out of a
  degenerate transform is refused rather than kept. Where that bites is not
  symmetric: the pixel floor is the only check that reads the source pixels,
  and the area floor alone backstops all three box floors, because a NaN in
  either linear fraction poisons the area. None of these is a format option
  (this design has no size or greyscale controls): they are documented
  behaviour with a rationale, like #7 and #9.
- **The cap:** `MAX_IMAGES = 60` per document, keeping a PREFIX in document
  order rather than the largest. A size-ranked subset illustrates page 3 and
  page 40 and nothing between, which reads as damage; a prefix reads as the
  art stopping. Over the cap the user is told so ("Screepub found 126
  pictures in this script. Only the first 60 were kept."). The only other
  picture warnings are pictures that could not be read, and decoders that
  could not be unpacked. The ordinary refusals above are never warned about,
  since nearly every real script has some. Separately, `IMAGE_BUDGET_MS =
  20000` bounds a document's whole image pass (resolve, convert, downscale,
  encode), so a document of distinct full-page pictures cannot spend half a
  minute on them. A document either guard refuses (#14, #15) never starts
  that pass: both are decided on the text alone, and the pictures are decoded
  only once both have passed (`extractText` in extract.ts).
- **A picture cannot split a speech or cost a cue its speaker.** The
  serializer's two neighbour tests look past pictures: cue demotion (a cue
  with no speech under it becomes action) and the `(CONT'D)` rejoin (#8) both
  read the nearest element that is not an image, so a plate between a cue and
  its speech, or in a page seam between a speech's halves, changes neither. A
  note that lands inside an open speech waits and is written right after the
  speech closes, a few lines lower than the PDF drew it, because a cue alone
  in its own paragraph is not a cue to fountain-js. And a picture never
  anchors a page marker (#13a): it is not a line of text a reader can land on.
  One imprecision is pinned rather than fixed: a picture printed inside a
  dual-dialogue region follows the LEFT speech, because the right column's y
  restarts at the top of the exchange (#10a). Both speeches survive, and
  fountain-js still reads one exchange.
- **JPEG 2000, JBIG2 and CCITT** are decoded by modules pdf.js loads from
  OUTSIDE its bundle, through `wasmUrl`; without them every such picture is
  lost silently. `src/parser/pdfjs-wasm.ts` embeds them and unpacks them into
  a private temp folder, asked for per document. A machine where that folder
  cannot be made still converts, and the warning says the cause is ours.
  `tests/fixtures/jpx.pdf` is the guard, and `tools/smoke-cli.ts` converts it
  with every compiled CLI it smokes and requires `imageCount` 1: the Linux
  binary on every push (ci.yml), Linux and Windows at a tag (release.yml).
- **Provenance of the numbers.** Every threshold was chosen by eye from the
  August design on 2026-08-06, against no real script, then **audited against
  the private corpus on 2026-08-12 (commit `1337863`), and none moved.** Seven
  documents (five feature screenplays, a 110-page script-plus-lookbook, a
  30-page pitch deck): 207 painted placements, 83 kept, 58 refused by these
  rules, 66 lost to the cap, every kept picture looked at, and no false
  positive or false negative in a document that passes the guards. The
  margins it left: area, the smallest kept picture a reader wanted is 3.30% of
  its page against a largest refusal of 2.54% (the smallest keep of any kind
  is 3.02%, pitch-deck decoration, so a retune has about half a point to clear,
  not 0.8); width, 13.1% kept against 9.6% refused; pixels, a smallest kept
  edge of 110 px against the one refusal, an 84x17 blank strip. Rule 5
  accounts for 19 of the 58 refusals and every one was right; rule 6 fired
  once, on a sheet of exactly 20 lines, which is support and not proof. The
  known blind spot is the hash: a producer that re-rasterizes its logo per
  page defeats it, and the size floors caught that case instead.
  `artwork.ts` keeps each of these beside its constant. `tests/fixtures/
  images.pdf` pins every rule, with each picture's margin to its one governing
  threshold stated in `tools/make-fixture.py`; a fixture can show the numbers
  are self-consistent, never that they are right.
- **Measured on the corpus, 2026-10-01: the `.fountain` gains notes and
  nothing else.** Every PDF in the local set converted with `origin/main` and
  with this phase, `.fountain` against `.fountain`. The only added lines are
  `[[img: ...]]` notes and the blank line after each; no line was removed or
  changed anywhere. Per script, anonymised in sorted order: scripts 1, 3, 4
  and 5 are byte-identical (no picture kept). Script 2 gains 1 note, a
  full-page plate on its second sheet (1 PNG, 1.4 MB). Script 6 is refused as
  not a screenplay (#15) on both sides; converted with `--force` to measure
  it, it gains 60 notes, 13 of them full-page (126 pictures found, 60 kept,
  the cap warning given; 60 PNGs, 15.8 MB). Script 7 is refused as scanned
  (#14) on both sides and writes no `.fountain`. **What it costs is time.**
  Scripts 1, 3, 4 and 5 convert within 0.01 s of before; script 2 goes from
  0.30 s to 0.60 s; forced script 6 from 1.6 s to 16.8 s, the worst case. A
  refusal costs what it cost before pictures existed, because the image pass
  runs only after both guards have passed: script 6 is refused as not a
  screenplay in 1.54 s (main 1.53 s), and script 7 as scanned in 0.09 s (main
  0.09 s), best of two. Run ahead of the guards, the same pass makes those
  16.7 s and 3.7 s.
- **Default:** always on. There is no knob in this phase; phase 2's
  `includeImages` will gate rendering, not recognition, so the notes stay in
  the `.fountain` whatever it is set to.
- **Code:** `src/parser/images.ts` (`collectImagePlacements`, `toRgb`,
  `IMAGE_MAX_EDGE`, `IMAGE_BUDGET_MS`, `isGeneratedImageFile`),
  `src/parser/artwork.ts` (every threshold, `keepArtwork`, `MAX_IMAGES`,
  `columnWidthPct`, `isFullPage`), `src/parser/png.ts`,
  `src/parser/pdfjs-wasm.ts`, `src/parser/extract.ts`, `src/parser/index.ts`
  (`insertImages`), `src/fountain/serialize.ts` (`imgNote`, `prevNonImage`,
  `nextNonImage`, `anchorsPageMarker`), `src/fountain/notes.ts`
  (`parseImgNote`, `isSafeImageFile`), `src/library-images.ts`,
  `src/convert.ts` (`imageWarnings`), `src/cli.ts`, `src/cli-errors.ts`.
