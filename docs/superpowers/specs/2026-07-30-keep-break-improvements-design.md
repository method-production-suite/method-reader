# Keep/break improvements: six small items from the renderer research

Date: 2026-07-30
Status: approved (brainstorm complete)
Branch: keep-break-batch (base on main: the 2026-07-30 merge train
landed in v0.4.2. Coordinate ordering with the rich-formatting spec,
also approved on main and also touching css.ts/html.ts territory)

## Goal

Batch the small, concrete improvements surfaced by the 2026-07-30
advanced-formatting research sweep (CSS fragmentation support across
engines; Kindle conditional formatting; production-practice survey) into
one branch. Six items: a widows/orphans knob, a third keep spelling,
MOBI scene breaks, a page-marker theme fix, the Apple Books
font/alignment meta, and the doc corrections the research demands.
Everything here is engine + docs + one app toggle; no parser changes.

## Decisions from the brainstorm

- **Scope:** the six items below, nothing else. Deliberately excluded:
  `amzn-*` conditional CSS blocks (the useful `amzn-et` variant does not
  survive our own ebook-convert pipeline, `amzn-kf8` also matches KFX so
  it cannot isolate old Kindles, and an empty block crashes legacy
  RMSDK: a future design of its own, if ever), the Apple Books heading
  wrapper (re-introduces the half-page-push problem registry #5a
  killed), and Courier Prime embedding (moved to ROADMAP.md "Later",
  done alongside this spec).
- **Knob shape (the owner's call):** widows/orphans ships as a toggle,
  default on at the print-rule value of 2; off means 1 (tight packing).
  Fits the registry convention that house-style rules get options.
- **Landing shape:** one branch, six ordered commits, one review.
- **Sequencing:** the 2026-07-30 merge train already landed (v0.4.2),
  so main is the base. One umbrella implementation plan sequences this
  batch (phase A) ahead of the rich-formatting spec's two phases: the
  batch builds the shared MOBI options plumbing and the knob-ritual
  worked example that rich-formatting phase 2 reuses, and one combined
  device pass settles every pending registry verdict.

## Item 1: print split minimums (widows/orphans knob)

- **Engine:** `FormatOptions.printSplitMinimums: boolean`, default
  `true`. CSS emission on `p.dialogue, p.action`:
  true → `widows: 2; orphans: 2`; false → `widows: 1; orphans: 1`.
  Always emitted explicitly (the point is to state the rule, not
  inherit whatever a device defaults to).
- **Why:** encodes pagination-reference §2's two-line split minimum
  (Movie Magic house rule) in the one CSS mechanism renderers actually
  honor for it. The research corrected the registry's belief that
  widows/orphans are dead: KFX honors them from fw 5.12.3 (Kindle
  Previewer 3.35 added them circa 2019) and Kobo/tolino RMSDK
  demonstrably honors book CSS (MobileRead t=328903, 2020). Ignored
  harmlessly on KF8/AZW3, MOBI, kepub e-ink. The `false` value is the
  community-documented trick for reclaiming bottom-of-page white space.
- **App:** `FormatSettings` field + reader-rail toggle in the Page
  group, label "Print-style split minimums", caption stating the
  trade (faithful splits vs tighter pages) and that it applies on
  KFX/Kobo/tolino only.
- **Pinning:** default lands in `format-defaults.json`,
  `options.test.ts`, and kit-check in the same commit (standing
  three-way rule).
- **Registry:** new entry at the next free number (#17 as of this
  writing) with device-support notes, sources, and a
  pending-device-verdict slot (next KFX pass, alongside #8c/#16).
- **Interaction:** `keepSpeechesWhole` on makes dialogue widows moot
  inside a kept speech; no conflict, note it in the registry entry.

## Item 2: third keep spelling for wrapper keeps

- **Engine (css.ts):** one new standalone rule:

  ```css
  .keep-together, table.dual-dialogue { -webkit-column-break-inside: avoid; }
  ```

- **Why:** Readium-family readers (Thorium, Kobo mobile apps) and
  kepub paginate via CSS multicolumn, where the old column spelling is
  the honored form. Must be a SEPARATE rule: iBooks has a documented
  bug (BlitzTricks) where this property and `page-break-inside` in the
  same declaration block are BOTH ignored.
- **Deliberate deviation from Blitz:** no `@supports` guard. Blitz
  guards this rule, but the engines that need the column spelling are
  old WebKit builds that largely predate `@supports`, so the guard
  hides the rule from its own audience. Unguarded is safe: an engine
  honoring both spellings reads the same value twice.
- **Scope note:** wrapper keeps only. The chain rules (`break-after`
  on headings/cues) have no column-spelling equivalent worth emitting
  (`-webkit-column-break-after: avoid` has no observed support in the
  target engines; skip it).

## Item 3: MOBI scene breaks

- **Engine (mobi dialect):** `tokensToMobiHtml` gains a FormatOptions
  parameter. When `scenePageBreaks` is true, emit `<mbp:pagebreak/>`
  immediately before each scene heading except the first.
- **Why:** `<mbp:pagebreak/>` is the ONLY fragmentation primitive the
  Mobipocket dialect has (MobileRead wiki). Today the MOBI path
  receives no break options at all (pagination-reference gap #6);
  this closes the scene-break half of that gap. Gap #7 (Fountain `===`
  acts as a forced break) stays out of scope.
- **Registry:** update the `scenePageBreaks` entry and the MOBI
  structural note in #8c ("no stylesheet" stays true; this is markup,
  not CSS).

## Item 4: page-marker theme fix

- **Engine (css.ts):** `span.page-marker` drops `color: #777777`,
  gains `opacity: 0.6`. Everything else unchanged.
- **Why:** a hardcoded gray fights themed backgrounds (dark, sepia).
  Opacity dims relative to the theme's own text color, so the marker
  recedes correctly everywhere. Engines without opacity support render
  it full-strength: a harmless degrade. Enhanced Typesetting lists
  opacity as supported (Guidelines 2026.2 §18.1).

## Item 5: Apple Books font and alignment meta

- **Engine (src/epub/build.ts):** the OPF gains
  `<meta property="ibooks:specified-fonts">true</meta>`, with the
  required prefix declaration on the package element
  (`prefix="ibooks: http://vocabulary.itunes.apple.com/rdf/ibooks/vocabulary-extensions-1.0/"`);
  epubcheck flags the property without the prefix.
- **Why:** without it, Apple Books' user Justify setting overrides our
  ragged-right `text-align` and Books ignores `font-family` choices.
  Already flagged as the highest-payoff tier-2 fix in device-map §5
  ("Now" item 2), and it future-proofs the rich-formatting spec's font
  classes. Books-only semantics; every other reader ignores the meta.
- **Registry:** note under #6b (justifyText) that this meta is what
  makes the knob hold in Apple Books; check the item off device-map §5.

## Item 6: doc corrections and new invariants

- **device-map.md §2.1:** rewrite the rendering paragraph. The claim
  "page-break-inside, widows, orphans all ignored under ET" comes from
  the Kindle Publishing Guidelines PDF (2026.2), whose Appendix B is
  stale and self-contradictory. Ground truth: the live KDP help pages
  list all break/keep properties with avoid as supported, Kindle
  Previewer 3.35/3.36 release notes show them arriving circa 2019, and
  jhowell's device tests plus our own #8b pass confirm KFX honors
  them. Keep the file-split advice for kepub e-ink; delete it for KFX.
  Also correct the same carried line in §5's registry-corrections
  block.
- **device-map.md, new section:** the property-by-engine fragmentation
  support matrix from the research (per its charter: "what each
  renderer honors"), with confidence levels and source links.
- **Two new invariants** (CLAUDE.md CSS bullet + css.ts header):
  1. No background-color on html/body, ever: it makes the KFX
     converter synthesize a wrapper block, and keeps are honored on
     top-level blocks only, so every keep dies silently (jhowell,
     MobileRead t=330798).
  2. CSS values stay 2.1-vintage: RMSDK violates CSS error handling
     and can blank the whole book on modern value functions like
     `min()` (Klein, 2026-06).
- **Registry #8c:** add the oversize-keep caveat to the pending device
  verdict: a kept block taller than one page historically made
  Previewer fail to render pages (jhowell, 2016) and is the blank-page
  bug Blitz specifically disables keeps on Kindle to avoid. The #8c
  pass must include the longest speech in the fixture set at the
  largest font size.
- **Registry #5/#8b:** note the third spelling (item 2) where the keep
  inventory is described.

## Testing

TDD per item, in the standing bun:test suite:

1. options.test.ts: `printSplitMinimums` default pinned; CSS emission
   asserted for both toggle states; the new `-webkit-column-break-inside`
   rule and page-marker opacity asserted.
2. MOBI test: `scenePageBreaks` on → `<mbp:pagebreak/>` count equals
   scene-heading count minus one; off → none.
3. kit-check: FormatSettings round-trip for the new field;
   format-defaults.json three-way pin.
4. OPF test: the ibooks meta and its prefix declaration present;
   epubcheck stays clean.
5. Fixture sweep + epubcheck (CSS change rule), end-to-end spot check
   with a real PDF, app bundle rebuild (engine changed).
6. Device verdicts stay pending until the owner's next KFX pass: the knob's
   entry, #8c with the oversize caveat, #16 (plus an Apple Books look
   at ragged-right holding with Justify on, for item 5).

## Out of scope

Parser changes of any kind; `amzn-*` media queries; Apple Books heading
wrapper; Courier Prime embedding (ROADMAP.md "Later"); Fountain `===`
forced breaks in MOBI (gap #7); kepubify-side changes; any change to
the AZW3/ebook-convert flag set.

## Sources

The three research reports (2026-07-30, this session): engine support
matrix, Kindle conditional-formatting deep dive, production-practice
survey. Load-bearing citations: KDP Text Guidelines (reflowable) help
topic GH4DRT75GWWAGBTU; Kindle Previewer release notes (3.35, 3.36);
MobileRead t=330798 (avoid is KFX-only; top-level blocks; background
wrapper trap), t=328903 (RMSDK widows/orphans), t=356002 (amzn-et,
conversion-time queries), t=346874 (kepub ignores break CSS);
kobolabs/epub-spec; BlitzTricks and Blitz framework (FriendsOfEpub);
Standard Ebooks drama how-to; clagnut.com/blog/2426 (WebKit lacks
break-after avoid); andreklein.net 2026-06-14 (RMSDK blanking);
MobileRead wiki: MOBI (mbp:pagebreak).
