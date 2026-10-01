# Design: images in the script, carried into the e-book (the rebuild)

Date: 2026-10-01 · Status: **APPROVED by the owner, 2026-10-01** ("design
approved fire away"), after answering the four questions in "Decisions for
the owner" with every recommendation: pictures on by default, the title
card as the cover by default, ship after the identifier release, and the
MOBI fallback carries pictures. The technical choices stand as written.
Phase 1's plan: `docs/superpowers/plans/2026-10-01-images-phase-1.md`.

Supersedes, as a plan of record: the August design on branch
`worktree-images` (spec 838da41, plan 7110fa7). That branch stays as
history. The owner decided on 2026-10-01 to rebuild the feature on today's
code rather than rebase it (60 commits, about 441 behind main).

## Goal

The ROADMAP's own words, in three parts, each useful alone:

1. Recognise that a page holds an image and where it sits.
2. Carry it into the e-book at a sensible size.
3. Promote a full-page image at the front into the book's actual cover.

Two limits are stated up front and not designed away: most e-ink screens
are greyscale, so a colour picture reads as black and white on a Kindle;
and a script padded with full-page photographs makes a file too big to
email to a `@kindle.com` address.

## What the August branch already settled

The old branch was built, reviewed, mutation-tested and measured against
the private corpus. Its knowledge is the main asset; its integration is
not. What carries over, with the commit to read:

- **Extraction** (`src/parser/images.ts`): one extra pass over the operator
  list extract.ts already fetches; placement by walking `q`/`cm`/`Q` and
  taking the bounding box of all four transformed corners, so flips and
  rotations come out right (e663c03). Form XObject matrices read; images in
  annotations skipped.
- **Pixels**: pdf.js's three image kinds (1-bit, RGB, RGBA flattened onto
  white), the async `objs.get` with `page.objs` and `page.commonObjs`
  raced under one timeout, and one 20-second budget per document for all
  image work (5a55098, 996874e, a692b18). Pixels must be read before
  `page.cleanup()` or they vanish without an error.
- **PNG encoder** (`src/parser/png.ts`): no new dependency, `node:zlib`,
  adaptive row filters, box downscale to a 1400-pixel long edge, verified
  pixel-exact through macOS ImageIO (b03203d).
- **Six filter rules** separating artwork from page furniture
  (`src/parser/artwork.ts`, 567a227): a 32-pixel floor, 10% of page width,
  3% of page height, 3% of page area, the same picture on 3 or more pages
  (or on 25% of a short document), and a near-full-page image behind 20 or
  more lines of text. Every drop is attributed to the rule that fired.
  **The corpus audit (1337863) moved no threshold**: 207 placements, 83
  kept, 58 refused, 66 lost to the cap. The area floor has 0.8 points of
  margin and should not be raised.
- **The Fountain note**: `[[img: p012-01.png w=71 full]]` on its own block,
  filenames `p%03d-%02d.png`, width clamped 5 to 100, and a filename
  allowlist that is a security boundary (b4a25a9). Notes are always
  written; the option only decides whether they render. That keeps the
  parser option-free, which is a project invariant.
- **The cue-and-speech trap** (81e32b5 to 09f8c3f): a picture between a
  character cue and its dialogue demoted the cue to action and scattered
  the speech. Fixed by looking past images in both directions and
  deferring the note until the speech closes. `showPageMarkers` now
  defaults ON, so this sits on the default path and its tests come first.
- **Sizes, measured**: one full-page plate took a book from 51 KB to
  1.5 MB; 60 plates came to 16.3 MB (about 270 KB each, not the 100 KB
  first guessed). Email to Kindle stops at 50 MB, so near 180 pictures.
  Hence `MAX_IMAGES = 60`, keeping the first 60 and warning (7187f6f).
- **Cover** (30392ed): the first full-page picture before any scene
  heading becomes the cover; EPUB declares both cover spellings; either
  one alone carries it into AZW3 and KFX; with no declaration KFX gets
  Kindle Previewer's grey placeholder.
- **MOBI image records** (92d232e): placed between text and FLIS, with the
  header offsets written down. Kindle Previewer refuses any third-party
  MOBI 6, and whether an old Kindle decodes a PNG record is unknown.

## What changed underneath it since August

Seven seams, each found by comparing the old branch with today's code:

1. **The Kindle fallback would strip every picture, silently.**
   `freshKindleArtifact` in `src/export/artifact.ts` builds the MOBI by
   calling `convertFountain(fountainText, { format, mobi: true })` with no
   images (checked 2026-10-01, line 111). On a machine without Calibre,
   "Save a Kindle file" and USB to a Kindle would ship a book with no
   pictures and no warning. Reading the picture folder has to be one
   shared function both the CLI and the export path call.
2. **The library has per-script folders now** (`src/library.ts`). The old
   `<stem>.images/` beside the `.fountain` still works mechanically,
   because the `.fountain` lives in the script's folder.
3. **The CLI grew a lot** (`--library`, settings sidecars, inline preview,
   `spacingRepairs`). The old `imagesPath`/`imageCount` JSON fields and the
   `unwritable` error code are re-ported by hand, and `unwritable` needs a
   heading in `desktop/ui/convert.js`.
4. **Settings are per script.** A first library conversion saves the app's
   defaults as that script's own settings, so the default for pictures is
   frozen into every new script. The default matters more than it did.
5. **The reader preview is an iframe under `default-src 'self'`**
   (`tauri.conf.json` line 24, `desktop/ui/read.js`). That blocks `data:`
   images AND inline `style=` attributes, so the old markup's
   `style="width:71%"` would not size anything in the preview.
6. **The page-break lesson (#8b)**: a property's verdict does not carry
   across DOM shapes, and KFX ignores keeps on a structural `<div>`. The
   old `.figure-full` put its breaks on a `div`.
7. **pdf.js went 6.2.108 to 6.3.289**, so every line citation in the old
   work is stale, and the engine now cross-compiles for Linux and Windows,
   where the JPEG 2000 and JBIG2 decoder folder handed to pdf.js has never
   been exercised.

The settings UI the old branch added was in the frozen Swift app and
cannot be re-applied; its two toggles move to the window's Tune page.

## Approaches

**A. Port forward, phase by phase (recommended).** Bring the pure modules
across almost unchanged (PNG encoder, filters, placement walk, note
parser, cover picker, MOBI records), because they were measured and their
files are untouched since August. Redo every integration seam above
against today's code, with today's tests. Ship in four phases, each one a
release that stands alone. Cost: the most work of the three, nearly all
of it in seams rather than algorithms. Benefit: nothing measured is
re-guessed, and every place the old work would have broken today gets a
test first.

**B. Text-first minimum.** EPUB pictures only, no MOBI pictures, no cover,
no toggle. Fastest to a first release, but it ships seam 1 as a known
silent loss on the Kindle fallback, and the cover (the part most readers
would actually notice in a library grid) waits.

**C. Rasterise whole pages that hold art.** Render the page with pdf.js and
drop the picture of the page in. Rejected: pdf.js rendering needs a canvas
the engine does not have headless, it would carry the page's text as
pixels beside the real text, and it throws away every filter the corpus
audit validated.

## The design (approach A)

**Data flow, unchanged in shape from August:**
PDF → extract (text plus image placements) → filters → elements with
`image` entries at their page and height → Fountain with `[[img: ...]]`
notes → pictures written to `<stem>.images/` beside the `.fountain` →
renderers read notes plus the folder → EPUB figures / MOBI records →
cover from the first eligible note.

**Units, each testable alone:**

| Unit | Job | Depends on |
| --- | --- | --- |
| `src/parser/images.ts` | placements and pixels from pdf.js | pdf.js only |
| `src/parser/artwork.ts` | the six rules, no pdf.js | nothing |
| `src/parser/png.ts` | encode and downscale | `node:zlib` |
| `src/fountain/notes.ts` | `parseImgNote` beside today's note stripping (registry #18 stays the one copy) | nothing |
| `src/library-images.ts` (new) | read and write the picture folder: lstat, size cap, PNG check, the 60 cap, prune only names it wrote | `atomic-write.ts` |
| `src/epub/`, `src/mobi/` | figures and records | notes, folder reader |
| `src/fountain/cover.ts` | pick the cover | notes |

`src/library-images.ts` is the fix for seam 1: `cli.ts` and
`export/artifact.ts` both call it, so no route can build a book without
the pictures the `.fountain` names.

**Technical choices made here** (overrule any in one line):

- **The picture folder stays `<stem>.images/` beside the `.fountain`**, in
  the library and beside an input alike: one rule, no special case.
- **Figure width by class, not inline style**: `figure w70` and so on, in
  steps of 5% (the note's integer width rounds to the nearest step), so the
  same markup sizes correctly in the CSP-locked preview and stays CSS 2.1
  for RMSDK. Twenty short rules beat a CSP exception.
- **The figure is a `<p class="figure">`, not a `<div>`**, per seam 6. The
  full-page break goes on that `<p>`. Device-check item.
- **The preview shows pictures**: `img-src 'self' data:` added to the
  window's CSP, nothing else loosened. The cost is a bigger JSON answer,
  bounded by the 60 cap and the 1400-pixel edge.
- **Colour is kept** in the book, because the same EPUB goes to Kobo,
  Apple Books and phones. Transparency is flattened onto white, as in
  August. No greyscale or size controls in this design.
- **Hand-placed pictures** in a `.fountain` are tolerated by the allowlist
  but not advertised as a feature.
- **Decoders unavailable** (JPEG 2000, JBIG2, notably on Windows and Linux
  until proven): those pictures are skipped with a warning naming them,
  never a failed conversion.
- **Email size**: the email-to-Kindle route warns when a book is over
  50 MB rather than letting Amazon drop it silently.

## Phases, each its own release

1. **Recognise.** Extraction, filters, PNG, the note, the picture folder,
   the JSON fields, the cue-and-speech fix. The book is unchanged, because
   nothing renders a note yet and the option does not exist yet; the
   `.fountain` gains notes, and the corpus diff must show nothing else
   moving.
2. **Carry.** EPUB figures, the shared folder reader in both callers, the
   two Tune toggles in the window, the preview CSP line. Pictures default
   per decision 1.
3. **Kindle.** MOBI image records per decision 4; AZW3 and KFX inherit
   EPUB pictures through Calibre and need only a device check.
4. **Cover.** Promotion, both declarations, the Tune toggle.

## Errors and limits

Budgets stop waiting, not working: the 20-second document budget and the
5-second per-image timeout end image work and keep the text. Every refused
or capped picture is counted in the JSON warnings with the rule that
refused it. An unwritable picture folder is the `unwritable` error code,
with a heading in the window. A sidecar folder holding something the
engine did not write (a symlink, a FIFO, a non-PNG, an oversize file) is
skipped, never followed.

## Testing

- **Fixtures**, regenerated by `tools/make-fixture.py`: `images.pdf` (five
  sheets, two pictures kept and four refused, each by a different rule:
  pixels, width, furniture, background; height and area are pinned in
  `tests/artwork.test.ts`), `jpx.pdf` (one JPEG 2000
  picture), and rows added to `torture.pdf` for registry #19, with
  `tools/torture-manifest.json` rows to match.
  `tests/fixture-stability.test.ts` will flag one trap: an embedded zlib
  stream compressed on macOS may not match ubuntu's, so the fixture
  either pins its compressed bytes or is excluded from the
  byte-for-byte check with a reason.
- **TDD per unit**, mutation checks on the filters and the folder guards,
  epubcheck on every rendered fixture.
- **The corpus diff** before and after each phase. Phase 1 must add only
  `[[img: ...]]` lines.
- **Cross-platform**: the extraction pass runs in the Linux CI job; the
  decoder folder gets a Windows smoke test before phase 1 ships.
- **Device pass** before phase 2 and 4 ship: a sideloaded KFX and AZW3
  with a figure and a cover, a Kobo if one turns up. Built with a freshly
  regenerated `torture.pdf`.

## Decisions for the owner

1. **Pictures on by default for new scripts?** Recommended: **on**. The
   filters refused every bit of furniture the audit found and kept the
   art; a reader who does not want them turns them off once. Off means the
   feature exists only for people who find the toggle.
2. **Title card as the cover, on by default?** Recommended: **on**. It is
   the most visible part of the feature in a library grid.
3. **Ship after the identifier release?** Recommended: **yes**. Until then
   the frozen Swift app ships against this engine with no toggle for
   pictures and no way to add one.
4. **The text-only MOBI fallback: carry pictures, or say it cannot?**
   Recommended: **carry them**. The records are built and documented, and
   it is the route a Kindle owner without Calibre gets.

## Not in this design

Greyscale conversion, size or quality controls, alt text beyond a fixed
"Illustration", per-picture choices, landscape inserts measured against
their own page size, and a library view. Each is a reasonable later piece;
none is needed for the three parts in the goal.

## Risks

- **A tall plate slices across pages** on Blink (1:2 spans two, 1:4
  three), and Kindle strips `max-height`. Accepted and noted; a
  device check says how bad it is on hardware.
- **The cover may not survive a USB sideload** to a Kindle's library grid:
  Amazon deletes covers of USB-sent non-Amazon books, deliberately on
  Colorsoft and newer (device map §2.1). The cover still shows inside the
  book. Worth a device check before promising it.
- **Re-rasterised logos defeat the furniture hash** (one deck in the audit
  redrew its logo on 23 of 30 pages). Rare; the cap bounds the damage.
