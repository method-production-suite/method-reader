# Screenplay pagination: break rules, page artifacts, and what reflow owes them

Researched 2026-07-30, three-source synthesis: (1) industry formatting
authorities (Story Sense, the Warner Bros.-derived SimplyScripts guide,
Final Draft's manual, Movie Magic's rule list, John August), (2) direct
measurement of real per-tool PDFs from the local fixtures set plus the
source code of Beat and Trelby and the official docs of the rest, (3) a
grounding sweep of Method Reader's own pipeline. Companion docs:
`screenplay-format-reference.md` (metrics and CSS), `device-map.md`
(what each renderer honors), `formatting-options-log.md` (the options
registry; entries cited as #N).

Confidence legend: **[A]** universally agreed rule · **[B]** house-style
or software-default variation · **[C]** shooting-script-only convention
· **[E]** measured from real PDFs · **[D]** official doc or source code.

## 1. The page as a unit

- [A] 12pt Courier, 10 chars/inch, six lines/inch, single-spaced. One
  page ≈ one minute (sitcom ≈ 30 seconds).
- [B] Lines per page is NOT one number: Warner Bros. style (0.5" top and
  bottom) says 60; modern spec practice (~1") lands 52-55; measured
  tools sit at exactly 55 on a 12.0pt grid — except Celtx, which uses a
  13.0pt grid for **51 lines/page**, and Trelby, which defaults to A4
  (61 lines; 57 on US Letter) [E]/[D]. The production origin of "56" is
  scheduling eighths: 7 lines x 8.
- [A] **The bottom margin is elastic by design**: it "varies, according
  to the rules for where it's permissible to break a page" (Story
  Sense). The break rules below outrank the page edge — which is why a
  reflowable edition loses nothing essential by dropping the page and
  keeping the rules.
- [A] Page number: top right, **with a trailing period** ("4."), and
  **no number on page 1**. Every measured tool agrees; Google-Docs
  scripts are the exception (bare "2", no period) [E]/[D].
  Method Reader's `PAGE_NUMBER_BARE /^\d+\.?$/` already accepts both
  (classify.ts:14).
- [C] Production pages add: scene numbers in BOTH margins, A/B page
  numbers (78A, 78B) once locked, colored-revision asterisks in the
  right margin, and headers carrying title/draft/date.

## 2. The break rules, by element

What correctly formatted print does at a page bottom. These are the
semantics our reflow must either preserve (keeps) or undo (rejoins).

**Scene headings** — [A] never the last line of a page; the heading
moves forward to stay with its scene. [B] How much must follow varies:
one complete sentence (Warner Bros.), two lines (Movie Magic, Beat
hard-codes 2), and a standalone establishing-shot heading is a
recognized exception. [A] The one universal exception: a heading may end
a page if another heading or shot opens the next.

**Dialogue** — [A] splits only between sentences, never mid-sentence
(Final Draft ships "Break Dialogue and Action at Sentences" on by
default). At the bottom: `(MORE)` on its own line; at the top of the
next page: the cue repeated with a continuation extension. [B] At least
two dialogue lines on each side of the split (Movie Magic; Fade In was
measured allowing a 1-line widow). [B] `(MORE)` placement varies:
cue indent (most tools), its own unique indent (Highland, x=214 between
dialogue and cue), or centered. [B] The extension's case is house
style: `(CONT'D)` (Final Draft, Highland, WriterDuet, Beat) vs
`(cont'd)` (Fade In, Trelby) — and one measured file carried BOTH.
[A] The parenthetical is not repeated after the split; a mid-speech
break falls BEFORE a parenthetical, never after it.

**Character cues** — [A] never the last line of a page (the canonical
widow). The cue carries its speech forward.

**Parentheticals** — [A] never page-final, never separated from the cue
above, never split internally. (Final Draft's default permits page-final
parentheticals, so real PDFs violate this one.)

**Action** — [A] prefer breaking between paragraphs; within one, only at
a sentence boundary. [B] Two-line minimums are studio style. Measured
reality: Final Draft, Fade In, Highland all break action only at
sentence ends [E]; **Beat and Celtx split action mid-sentence** (Beat at
the exact fitting offset by default; Celtx measured with 17 of 34
spanning paragraphs ending mid-sentence) [D]/[E].

**Transitions** — [A] one-directional: a transition may END a page
(normal) but never BEGIN one; it belongs to the shot before it.

**TV acts** — [A] a new act always opens on a fresh page (`ACT ONE`
centered, underlined); never a break just before an `END OF ACT`
indicator. Sitcoms: never break between a heading and its cast list.

## 3. The artifact zoo: what tools actually emit

The strings a parser meets in the wild, with the traps [E] unless noted:

| Tool | Page-split markers | Trap |
|---|---|---|
| Final Draft | `(MORE)` at cue indent; `NAME (CONT'D)` | Straight `'` in the marker while body text uses curly `'` (smart quotes on by default). Continuation pages start ONE LINE HIGHER (body top 59.5pt vs 71.5pt). Most `(CONT'D)`s are NOT page breaks (56 of 66 measured were mid-page action-interruption continueds) |
| Fade In | `(MORE)` at cue indent; `NAME (cont'd)` lowercase | Same straight-vs-curly split; one file measured with 61 uppercase + 51 lowercase variants mixed |
| Highland | `(MORE)` at its OWN indent; `NAME (CONT'D)` with CURLY apostrophe | No character-level auto-continueds at all (deliberate); continuation pages do not shift |
| Beat | `(MORE)`, ` (CONT'D)` [D] | Strings are user-editable prefs; action splits mid-sentence by default; page furniture lives in table cells (fragmented text runs) |
| Trelby | `(MORE)`, ` (cont'd)` lowercase [D] | Defaults to A4; scene numbers drawn in both margins when enabled |
| WriterDuet | `(MORE)`, `(CONT'D)` on by default [D] | Widow protection ("Optimize splitting") is OFF by default; PDF export offers copy-protection and watermarks that pollute extraction |
| Arc Studio | auto-`(CONT'D)` OFF by default [D] | `(MORE)` string undocumented |
| Celtx | **NOTHING — all markers off by default** | Dialogue guillotined silently mid-block (9 unmarked spans in 99 measured pages); 13pt grid, 51 lines; action breaks mid-sentence; free tier stamps a footer on every page |
| Google Docs | nothing, ever | Bare page numbers without the period; dialogue guillotines |
| Browser-printed (WriterDuet/Arc class) | `(MORE)`; `NAME (CONT'D)` curly | Chromium/Skia fingerprint: U+2011 non-breaking hyphen for every hyphen, U+A789 for every colon, sub-pixel 11.9/12.0 leading drift |

Union a tolerant matcher must accept: `(MORE)`, `(CONT'D)`, `(cont'd)`,
`(CONTINUED)`, `CONTINUED:`, `CONTINUED: (n)`, `CONTINUED: n` —
case-insensitive, straight AND curly apostrophe, remembering every one
of these strings is a user-editable free-text field in every tool.

**Scene CONTINUEDs are off by default everywhere** (Final Draft FDX
default `SceneBottomOfPage="No"`; Trelby `sceneContinueds=False`; Celtx
off; Beat has no code for them at all) — they appear only in production
drafts, exactly as the industry sources say [C]. When present:
`(CONTINUED)` bottom-right, `CONTINUED:` top-left with the scene number
and optionally a run count (`CONTINUED: (2)` — Celtx: `CONTINUED: 2`).

**Export semantics:** mores/continueds are render-time artifacts
recomputed at print and never stored — FDX carries only the settings,
Fountain has no representation for them at all (its only pagination
construct is `===` forced break). In extracted PDF text they are
pagination noise to strip and rejoin.

## 4. Semantic vs residue: the (CONT'D) fork

The single most important disambiguation in this domain:

- **Page-break `(CONT'D)`** (cue repeated after `(MORE)`): pure layout
  residue. The speech is ONE element; the industry sentence-boundary
  rule makes rejoining safe. Drop the marker, merge the halves.
- **Action-interruption `(CONT'D)`** (same speaker resumes after action
  within a scene): semantic, kept in every draft, useful to actors and
  readers. Must survive conversion — regenerated on output if desired.
- **Scene `CONTINUED:`**: always residue, never semantic.

Method Reader already implements this fork correctly for the marked case:
the serializer drops a `(CONT'D)` cue only when it directly follows the
same character's speech (serialize.ts:73-87, page-break case) and keeps
it after intervening action (the semantic case), while `contdMode: auto`
regenerates clean markers on output (#8, #8a). The industry research
confirms this design; no change needed.

## 5. What Method Reader handles today

Condensed; full mechanics in the grounding notes and registry entries.

- Page furniture: numbers (bare/dashed/labeled), recurrence-based
  headers/footers, revision stars, dual-margin scene numbers —
  recognized and stripped or re-typed (#7, #9, #9a, #10;
  classify.ts:14-18, boilerplate.ts, extract.ts:83, extract.ts:295).
- `(MORE)`: dropped at serialization regardless of classified type
  (serialize.ts:14, :71). `(CONT'D)`: both apostrophes, optional
  period, case-insensitive (serialize.ts:15); rejoin gated by
  `rejoinSplitDialogue` (default on).
- Final Draft double-printed markers deduplicated geometrically
  (extract.ts:144-156, #10); a `(CONT'D)` printed 1pt above its cue
  absorbed by Y_TOLERANCE (extract.ts:68).
- Output keeps are CHAINS on the paragraphs, not wrappers: `break-after:
  avoid` on the heading (#5a), on the cue and on the parenthetical (#8b),
  plus the unbreakable dual-dialogue table (#10a). Both wrappers were
  deleted after they stranded what they were meant to hold — the heading's
  in 2026-07-30, the cue's in 2026-09-14 — because the KFX converter
  ignores break-after on a structural div. Chunks stay small via `orphans`
  (#17) rather than by leaving links out (#5). Device reality: KFX honors keeps; sideloaded
  AZW3/MOBI does not (#8b); Kobo/RMSDK need file splits for hard breaks
  (device-map.md §2).
- Original print pages can be surfaced as `= pg N` markers +  EPUB3
  page-list (`showPageMarkers`, #13a), offset-corrected against title
  pages.

## 6. Gaps the research exposes

Recognition (parser — stays option-free per the registry note):

1. **`CONTINUED:` / `(CONTINUED)` / `CONTINUED: (n)` are handled
   nowhere.** No regex, test, or fixture. A production-draft PDF would
   leak them into action or mini-slug elements. The recurrence layer
   won't catch the numbered variants (text varies per page). Celtx's
   parenless `CONTINUED: 2` shape included.
2. **Unmarked page splits (Celtx, Google Docs) guillotine dialogue and
   action silently.** Method Reader merges unmarked same-speaker dialogue
   into one Fountain block, but the halves stay separate rendered
   paragraphs (epub/html.ts:167-169); split action paragraphs are never
   rejoined at any stage (group.ts:48 creates them). Candidate
   heuristic, justified by §2: a block ending WITHOUT sentence-final
   punctuation at a page boundary, continued by a lowercase-starting
   block of the same type, is one element. (Celtx's 13pt/51-line grid
   and missing smart punctuation are corroborating fingerprints.)
3. **Continuation-page geometry:** Final Draft and Fade In start
   continuation pages one line higher than normal pages. Indent-based
   classification is immune, but any future body-top assumption isn't.
4. The `(CONT'D)` matcher should also accept Final Draft's straight
   apostrophe inside otherwise-curly documents — it already does
   (both codepoints in serialize.ts:15). No change; recorded because
   the mixed-apostrophe trap is easy to reintroduce.

Reconstruction and output:

5. **Transitions can begin a "page" in our EPUB.** Print rule: never.
   A `break-before: avoid` on transitions (or wrapping the preceding
   block) would mirror the cue/heading keeps — same caveat as all
   keeps: KFX-only in practice.
6. **MOBI output receives no break options at all** (`tokensToMobiHtml`
   takes no FormatOptions; no keep-together in the dialect). Structural;
   worth stating in the options registry so nobody expects keeps there.
7. TV act breaks: Fountain's `===` maps to a real spine/file split
   today only via scene packing; an act-aware split would satisfy the
   only [A]-grade hard-break rule in the domain.

Tests and fixtures:

8. No fixture exercises: `CONTINUED:` sluglines, an unmarked mid-block
   page split (Celtx-style), or an action paragraph split across pages.
   All current `(MORE)`/`(CONT'D)` tests are synthetic element arrays;
   `cargo xtask make-fixture` could generate the three missing shapes.

Doc hygiene: `screenplay-format-reference.md` §4's second open item
predates the `keepSceneHeadingWithScene` toggle (shipped, default on)
and is updated alongside this doc.

## Sources

Industry: Story Sense Screenplay Format Guide (storysense.com);
SimplyScripts WB-style format page; Final Draft 11 manual (Mores and
Continueds; Page Layout) and finaldraft.com format guide;
screenwriting.info page-breaking guide; scriptwritingsecrets.com
PageBreak (Movie Magic rule list); talentville.com page-break rules;
johnaugust.com (lines-per-page, how-to-cut-pages, cont'd-vs-continuous);
screenwriting.io on MORE/CONT'D; fountain.io/syntax; Nicholl rules PDF.
Software: pristine Final Draft 13 FDX defaults; Beat and Trelby source;
Fade In, Highland, WriterDuet, Arc Studio, Celtx official docs;
per-tool geometry and codepoint measurements from the local fixtures
set (no titles or authors reproduced here by standing repo rule).
