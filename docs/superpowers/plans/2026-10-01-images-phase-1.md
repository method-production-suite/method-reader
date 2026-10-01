# Images in the book, phase 1 (Recognise): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recognise every picture in a screenplay PDF, keep the artwork and refuse the page furniture, and record each kept picture in the `.fountain` as an `[[img: ...]]` note with its PNG written to `<stem>.images/` beside it. The book does not change yet.

**Architecture:** Port the August `worktree-images` branch's measured, mutation-tested modules (PNG encoder, the six filters, the operator-list walk, pixel decoding, pdf.js decoders) onto today's code, and redo the integration seams today's code moved: extraction, classification, the conversion entry point, the CLI, and a picture-folder module (`src/library-images.ts`) that every caller shares. Spec: `docs/superpowers/specs/2026-10-01-images-in-the-book-design.md` (owner-approved 2026-10-01).

**Tech Stack:** Bun + TypeScript, pdf.js (`pdfjs-dist` 6.3.289), `node:zlib`, `bun:test`, Python 3 for `tools/make-fixture.py`.

---

## How porting works in this plan

The August code is in git. "Port" means bring a file or a diff across
exactly, with a named command, and then run its tests. It is not
"re-write it in the same spirit".

- **SOURCE** is branch `worktree-images` (tip `d2df8db`, 2026-08-12).
  **BASE** is `658a2acf`, where that branch left main.
- **Whole new files** come from SOURCE's final state, because later
  August commits (`b00ae0f`, `cde2a2d`, `7bd5e7a`) fixed them:
  `git show worktree-images:<path> > <path>`.
- **Files unchanged on main since BASE** take the August diff whole:
  `git diff 658a2acf worktree-images -- <path> | git apply`. Checked
  2026-10-01: `src/parser/types.ts`, `src/parser/index.ts`,
  `src/parser/pdfjs.d.ts`, `src/fountain/notes.ts`,
  `src/fountain/serialize.ts`, `tests/notes.test.ts`,
  `tests/serialize.test.ts`, `tests/parser-pipeline.test.ts` have no
  commits on main since BASE. If `git apply` refuses, stop and report: the
  premise has changed.
- **Files main has changed since BASE** (`src/parser/extract.ts`,
  `src/parser/classify.ts`, `src/convert.ts`, `src/cli.ts`,
  `src/cli-errors.ts`, `tools/make-fixture.py`,
  `tests/fixture-stability.test.ts`, `tests/cli.test.ts`) are ported by
  hand, hunk by hunk, as each task says.
- **Never port phase 2+ code** in this plan: nothing that renders a
  figure (`src/epub/`, `src/mobi/`), the `includeImages` or
  `promoteCoverImage` options, `src/fountain/cover.ts`, or the Swift app.
  Where an August test file mixes phases, the task names which tests to
  take.
- pdf.js moved from 6.2.108 to 6.3.289 since August, so line citations in
  August comments are stale. Do not "fix" them by guessing; leave a
  comment's pdf.js line numbers out rather than wrong.

Run every command from the repository root. Work on a branch named
`images` cut from `origin/main`. The owner's decision: this ships AFTER
the identifier release (F2), so it merges after F2 does.

## File structure

| File | Status | Responsibility |
| --- | --- | --- |
| `src/parser/png.ts` | new, ported | PNG encode, box downscale, `isPng` |
| `src/parser/artwork.ts` | new, ported | the six filter rules, `MAX_IMAGES`, column width, full-page test |
| `src/parser/images.ts` | new, ported | operator-list placement, pixel resolution and conversion, the per-document run and budget |
| `src/parser/pdfjs-wasm.ts` | new, ported | JPEG 2000 and JBIG2 decoders handed to pdf.js |
| `src/parser/pdfjs.d.ts` | August diff | the pdf.js typings the above use |
| `src/parser/types.ts` | August diff | `ImageRef`, `RawImage`, `img?` and `y?` on elements |
| `src/parser/extract.ts` | hand-ported | the extra image pass beside today's text pass |
| `src/parser/classify.ts` | hand-ported | carry each block's `y` onto its element |
| `src/parser/index.ts` | August diff | `insertImages`: pictures join the element stream at their page and height |
| `src/fountain/notes.ts` | August diff | `parseImgNote`, `isSafeImageFile` (the filename allowlist) |
| `src/fountain/serialize.ts` | August diff | write the note; keep a cue and its speech together around a picture |
| `src/library-images.ts` | new, adapted from `src/cli-sidecar.ts` | read, write and prune `<stem>.images/` |
| `src/convert.ts` | hand-ported | carry encoded pictures out of `convertPdf` |
| `src/cli.ts`, `src/cli-errors.ts` | hand-ported | write the folder, `imagesPath`/`imageCount` in `--json`, the `unwritable` code |
| `desktop/ui/convert.js` | one line | a heading for `unwritable` |
| `tools/make-fixture.py` | hand-ported | `images.pdf` and `jpx.pdf` |
| `tests/fixtures/images.pdf`, `tests/fixtures/jpx.pdf` | regenerated | five sheets, six pictures; one JPEG 2000 picture |

---

### Task 1: The image fixtures

**Files:**
- Modify: `tools/make-fixture.py`
- Create: `tests/fixtures/images.pdf`, `tests/fixtures/jpx.pdf` (generated)
- Modify: `tests/fixture-stability.test.ts`

- [ ] **Step 1: See the August generator changes and today's file side by side**

```bash
git diff 658a2acf worktree-images -- tools/make-fixture.py > /tmp/images-make-fixture.diff
git diff 658a2acf origin/main -- tools/make-fixture.py > /tmp/main-make-fixture.diff
grep -n "^@@" /tmp/images-make-fixture.diff /tmp/main-make-fixture.diff
```

The August diff turns `build()`'s output into a `bytearray` (binary image
streams cannot pass through `latin-1` text) and adds the `images` and
`jpx` kinds. Main's diff since BASE adds the `demo` kind (Field Station).

- [ ] **Step 2: Port the August hunks into today's `tools/make-fixture.py`**

Apply every August hunk EXCEPT the `torture` additions (`imTall`, the
letterhead, the w=42 panel): those are phase 2's proof sheet. Keep main's
`demo` kind working through the `bytearray` change.

- [ ] **Step 3: Prove the existing fixtures did not move**

```bash
python3 tools/make-fixture.py   # however main's script is invoked; read its usage first
git status --short tests/fixtures/
```

Expected: `screenplay.pdf`, `prose.pdf`, `blank-pages.pdf` and
`field-station.pdf` show NO change. `torture.pdf` may change only if the
script always rebuilds it; if it changes, compare with
`git diff --stat` and stop if anything but its creation date moved.

- [ ] **Step 4: Generate the two new fixtures and pin them**

Produce `tests/fixtures/images.pdf` and `tests/fixtures/jpx.pdf`. Then
port the August additions to `tests/fixture-stability.test.ts`:

```bash
git diff 658a2acf worktree-images -- tests/fixture-stability.test.ts
```

Known trap (from the August notes): `images.pdf` embeds zlib output, and
CI runs on ubuntu while fixtures are authored on macOS. Run
`bun test tests/fixture-stability.test.ts` locally; if `images.pdf` does
not reproduce byte for byte across zlib builds, pin it the way August did
(read its test) rather than loosening the check for the other files.

- [ ] **Step 5: Bring the fixture tests across and run them**

```bash
git show worktree-images:tests/images-fixture.test.ts > tests/images-fixture.test.ts
git show worktree-images:tests/jpx-fixture.test.ts > tests/jpx-fixture.test.ts
bun test tests/fixture-stability.test.ts
```

`images-fixture` and `jpx-fixture` import modules from Tasks 2 to 5, so
they cannot run yet: commit them now and they go green at Task 5. Do not
skip them.

- [ ] **Step 6: Commit**

```bash
git add tools/make-fixture.py tests/fixtures/images.pdf tests/fixtures/jpx.pdf tests/fixture-stability.test.ts tests/images-fixture.test.ts tests/jpx-fixture.test.ts
git commit -m "Images: the fixtures, five sheets with one picture per rule, and one JPEG 2000"
```

### Task 2: The PNG encoder

**Files:**
- Create: `src/parser/png.ts`
- Test: `tests/png.test.ts`

- [ ] **Step 1: Bring the tests across first, and watch them fail**

```bash
git show worktree-images:tests/png.test.ts > tests/png.test.ts
bun test tests/png.test.ts
```

Expected: FAIL, cannot resolve `../src/parser/png`.

- [ ] **Step 2: Port the module**

```bash
git show worktree-images:src/parser/png.ts > src/parser/png.ts
bun test tests/png.test.ts
```

Expected: PASS. If a test shells out to `sips` (the ImageIO round trip),
it runs on macOS only; confirm it ran, not skipped, on a Mac.

- [ ] **Step 3: Commit**

```bash
git add src/parser/png.ts tests/png.test.ts
git commit -m "Images: the PNG encoder, ported (no dependency, adaptive filters, box downscale)"
```

### Task 3: The six filter rules

**Files:**
- Create: `src/parser/artwork.ts`
- Test: `tests/artwork.test.ts`

- [ ] **Step 1: Tests first, failing**

```bash
git show worktree-images:tests/artwork.test.ts > tests/artwork.test.ts
bun test tests/artwork.test.ts
```

Expected: FAIL, cannot resolve `../src/parser/artwork`.

- [ ] **Step 2: Port the module**

```bash
git show worktree-images:src/parser/artwork.ts > src/parser/artwork.ts
bun test tests/artwork.test.ts
```

Expected: PASS. The constants must arrive exactly as audited:
`MIN_PIXELS = 32`, `MIN_WIDTH_FRAC = 0.10`, `MIN_HEIGHT_FRAC = 0.03`,
`MIN_AREA_FRAC = 0.03`, `FURNITURE_PAGES = 3`,
`FURNITURE_SHARE_MIN_PAGES = 2`, `FURNITURE_FRAC = 0.25`,
`BACKGROUND_AREA_FRAC = 0.90`, `BACKGROUND_TEXT_LINES = 20`,
`FULL_PAGE_AREA_FRAC = 0.70`, `TEXT_COLUMN_FRAC = 0.706`,
`MAX_IMAGES = 60`. Check with
`grep -nE "^(export )?const [A-Z_]+ = " src/parser/artwork.ts`.

- [ ] **Step 3: Commit**

```bash
git add src/parser/artwork.ts tests/artwork.test.ts
git commit -m "Images: the six rules that separate art from furniture, ported with their audited constants"
```

### Task 4: The types

**Files:**
- Modify: `src/parser/types.ts`, `src/parser/pdfjs.d.ts`

- [ ] **Step 1: Apply the August diffs**

```bash
git diff 658a2acf worktree-images -- src/parser/types.ts src/parser/pdfjs.d.ts | git apply
git diff --stat
```

Expected: `types.ts` gains `ImageRef` (`file`, `widthPct`, `fullPage`),
`RawImage` (`file`, `pageNum`, `y`, `pageWidthPct`, `pageHeightPct`,
`bytes`, plus what `b00ae0f` added), `'image'` in the element type, and
optional `img?` and `y?` on `ScreenplayElement`.

- [ ] **Step 2: Typecheck**

```bash
bunx tsc --noEmit
```

Expected: clean. The new fields are optional, so nothing else breaks.

- [ ] **Step 3: Commit**

```bash
git add src/parser/types.ts src/parser/pdfjs.d.ts
git commit -m "Images: the element and raw-image types"
```

### Task 5: Placement, pixels and decoders

**Files:**
- Create: `src/parser/images.ts`, `src/parser/pdfjs-wasm.ts`
- Test: `tests/images.test.ts` (and Task 1's `images-fixture`, `jpx-fixture`)

- [ ] **Step 1: Tests first, failing**

```bash
git show worktree-images:tests/images.test.ts > tests/images.test.ts
bun test tests/images.test.ts
```

Expected: FAIL, cannot resolve `../src/parser/images`.

- [ ] **Step 2: Reconcile `OpList` before porting**

Today's `src/parser/extract.ts` already uses an `OpList` type
(`repairPhantomSpaces(items, ops: OpList | null)`), and August's
`images.ts` exports one too. There must be ONE definition:

```bash
grep -n "OpList" src/parser/*.ts
git show worktree-images:src/parser/images.ts | sed -n 40,50p
```

If the two shapes agree, `images.ts` keeps its export and `extract.ts`
imports it. If they differ, keep the wider one in one place and import it
in both. Do not keep two definitions.

- [ ] **Step 3: Port the modules**

```bash
git show worktree-images:src/parser/images.ts > src/parser/images.ts
git show worktree-images:src/parser/pdfjs-wasm.ts > src/parser/pdfjs-wasm.ts
bun test tests/images.test.ts tests/images-fixture.test.ts tests/jpx-fixture.test.ts
```

Expected: `images.test.ts` PASS. The two fixture tests drive extraction
end to end and pass once Task 6 is done; if they import
`extractDocument`, expect them to fail here, and say so in the commit.

- [ ] **Step 4: Check the pdf.js asset names against 6.3.289**

`pdfjs-wasm.ts`'s `DECODER_FILES` names files inside `pdfjs-dist`:

```bash
ls node_modules/pdfjs-dist/wasm/
git show worktree-images:src/parser/pdfjs-wasm.ts | sed -n 90,112p
```

Every file `DECODER_FILES` names must exist. If one was renamed, update
the name and add a test asserting each named file exists on disk.

- [ ] **Step 4b: The decoder folder on Windows**

`materializeDecoders` returns `` `${dir}/` `` and pdf.js appends a file
name to it. On Windows `dir` is a backslash path, and the JavaScript
fallback decoders are loaded by a dynamic `import()`, which may require a
`file:` URL rather than a drive path. Find out, do not guess:

```bash
grep -n "wasmUrl" node_modules/pdfjs-dist/legacy/build/pdf.mjs node_modules/pdfjs-dist/build/pdf.mjs | head -20
```

Read how 6.3.289 turns `wasmUrl` into a `.wasm` read and into an
`import()` under Node. Then make `materializeDecoders` build the string
both uses accept on every platform (likely
`pathToFileURL(dir).href + '/'` if both go through URL handling, or the
path with its native separator if `.wasm` is read with `fs`), and add a
test that builds it with `path.win32` for a `C:\\...` directory and
asserts the exact string. Task 10 runs it on a real Windows machine.

- [ ] **Step 5: Commit**

```bash
git add src/parser/images.ts src/parser/pdfjs-wasm.ts tests/images.test.ts
git commit -m "Images: placement from the operator list, pixel resolution, and the pdf.js decoders, ported"
```

### Task 6: Extraction runs the image pass

**Files:**
- Modify: `src/parser/extract.ts`
- Test: `tests/images-fixture.test.ts`, `tests/jpx-fixture.test.ts`, plus a new test in `tests/parser-pipeline.test.ts` (Task 7 brings the rest)

- [ ] **Step 1: Read what August changed and what today has**

```bash
git diff 658a2acf worktree-images -- src/parser/extract.ts
sed -n 20,62p src/parser/extract.ts
```

Today's `extractDocument` returns
`{ lines, pageCount, spacingRepairs }`. August's returned
`ExtractResult { lines, images, imageStats, pageCount }` and ran
`collectPageImages` inside the page loop on the SAME operator list the
text pass already fetches, before `page.cleanup()` (pixels vanish after
it), under one document-wide `ImageRun` from `newImageRun()`, finishing
with `finishImages` and the decoders from `pdfjsWasmDir()`.

- [ ] **Step 2: Write the failing test of the new contract**

Add to `tests/parser-pipeline.test.ts`:

```ts
import { extractDocument } from '../src/parser/extract';

describe('extraction returns pictures beside the text', () => {
  test('the images fixture keeps the art and refuses the furniture', async () => {
    const bytes = new Uint8Array(await Bun.file('tests/fixtures/images.pdf').arrayBuffer());
    const result = await extractDocument(bytes);
    // Today's fields are unchanged...
    expect(result.pageCount).toBe(5);
    expect(typeof result.spacingRepairs).toBe('number');
    // ...and the pictures come with them: the title card (sheet 1) and the
    // panel (sheet 2), named the way the note will name them.
    expect(result.images.map((i) => i.file)).toEqual(['p001-01.png', 'p002-01.png']);
    expect(result.images.map((i) => i.pageNum)).toEqual([1, 2]);
    // Every refusal is attributed to the rule the fixture built for it
    // (August's tests/images-fixture.test.ts, same numbers).
    expect(result.imageStats.droppedBy).toEqual({
      pixels: 1, width: 1, height: 0, area: 0, furniture: 5, background: 1,
    });
  });
});
```

Run it: FAIL, `images` is undefined.

- [ ] **Step 3: Wire the image pass into today's loop**

Port the August hunks into today's `extractDocument`, keeping
`spacingRepairs` and `repairPhantomSpaces` exactly as they are. The
return type becomes:

```ts
): Promise<{
  lines: RawLine[];
  pageCount: number;
  spacingRepairs: number;
  images: RawImage[];
  imageStats: ImageStats;
}>
```

Order inside the loop: fetch the operator list once; the text pass
(today's code) and `collectPageImages` both read it; resolve pixels
BEFORE `page.cleanup()`.

- [ ] **Step 4: Run the extraction tests**

```bash
bun test tests/parser-pipeline.test.ts tests/images-fixture.test.ts tests/jpx-fixture.test.ts tests/parser.test.ts
```

Expected: PASS. `parser.test.ts` must be untouched by this change: text
extraction did not move.

- [ ] **Step 5: Prove the text did not move on the committed fixtures**

```bash
bun src/cli.ts tests/fixtures/screenplay.pdf -o /tmp/p1-screenplay.epub --fountain /tmp/p1-screenplay.fountain
bun src/cli.ts tests/fixtures/field-station.pdf -o /tmp/p1-field-station.epub --fountain /tmp/p1-field-station.fountain
bun src/cli.ts tests/fixtures/torture.pdf -o /tmp/p1-torture.epub --fountain /tmp/p1-torture.fountain
```

Run the same three in a checkout of `origin/main` (writing to
`/tmp/main-*.fountain`) and `diff` each pair. No line may differ: the
`[[img: ...]]` notes do not exist until Task 8.

- [ ] **Step 6: Commit**

```bash
git add src/parser/extract.ts tests/parser-pipeline.test.ts
git commit -m "Images: extraction runs the image pass on the same operator list, before cleanup"
```

### Task 7: Pictures join the element stream

**Files:**
- Modify: `src/parser/index.ts` (August diff), `src/parser/classify.ts` (by hand)
- Test: `tests/parser-pipeline.test.ts` (August diff)

- [ ] **Step 1: Apply the August test diff, watch it fail**

```bash
git diff 658a2acf worktree-images -- tests/parser-pipeline.test.ts | git apply
bun test tests/parser-pipeline.test.ts
```

Expected: FAIL in the new `insertImages` tests. If any new test asserts
EPUB or MOBI output, delete that test (phase 2) and note it in the
commit.

- [ ] **Step 2: Carry `y` in classify.ts**

```bash
git diff 658a2acf worktree-images -- src/parser/classify.ts
```

It adds `y: block.yPosition` where an element is made from a block. Today's
`classify.ts` was restructured (73 lines changed since BASE), so find the
equivalent construction site(s) and add the same field there.

- [ ] **Step 3: Apply the element-stream diff**

```bash
git diff 658a2acf worktree-images -- src/parser/index.ts | git apply
bun test tests/parser-pipeline.test.ts tests/parser.test.ts tests/cue-text.test.ts
```

Expected: PASS. `parseLines(lines, images = [])` merges pictures by page
ascending, then `y` descending, the line first on a tie, AFTER
`mergeSplitSpeeches`.

- [ ] **Step 4: The dual-dialogue exception**

August pinned "a picture in the right column of a dual exchange uses the
left column's y" (commit `a00fcb6`). Today's `cue.ts` changed dual
detection. Run:

```bash
bun test tests/parser-pipeline.test.ts -t dual
```

If the pinned test fails, read `a00fcb6`'s message, decide whether
today's dual elements still carry the left column's `y`, and fix the
code, not the test.

- [ ] **Step 5: Commit**

```bash
git add src/parser/index.ts src/parser/classify.ts tests/parser-pipeline.test.ts
git commit -m "Images: pictures join the element stream where they sat on the page"
```

### Task 8: The `[[img: ...]]` note, and the cue-and-speech guard

**Files:**
- Modify: `src/fountain/notes.ts`, `src/fountain/serialize.ts` (August diffs)
- Test: `tests/notes.test.ts`, `tests/serialize.test.ts` (August diffs)

- [ ] **Step 1: Tests first**

```bash
git diff 658a2acf worktree-images -- tests/notes.test.ts tests/serialize.test.ts | git apply
bun test tests/notes.test.ts tests/serialize.test.ts
```

Expected: FAIL on the new tests (`parseImgNote` undefined; no note
written).

- [ ] **Step 2: Apply the code diffs**

```bash
git diff 658a2acf worktree-images -- src/fountain/notes.ts src/fountain/serialize.ts | git apply
bun test tests/notes.test.ts tests/serialize.test.ts
```

Expected: PASS. Confirm the exact note shape with one test's output:
`[[img: p012-01.png w=71 full]]`, on its own block with a blank line on
each side; `w=` an integer clamped 5 to 100; the filename allowlist
`^[A-Za-z0-9._-]{1,64}\.png$`.

- [ ] **Step 3: The default path: page markers are ON today**

August's cue-and-speech bug hid because `showPageMarkers` defaulted off.
It defaults ON now (`format-defaults.json`), so the August tests written
for that case are the ones that matter most:

```bash
bun test tests/serialize.test.ts -t "markers on"
bun test tests/serialize.test.ts -t "an image between a cue and its speech"
bun test tests/serialize.test.ts -t "never anchors a page marker"
```

Expected: PASS. Then prove they can fail: in `src/fountain/serialize.ts`,
make `anchorsPageMarker` return `true` for an `image` element, run the
first and third commands again (expected: FAIL), and restore it. Then
make the cue lookahead stop skipping images (replace the call to
`nextNonImage` with the plain next element), run the second command
(expected: FAIL), and restore it. `git diff src/fountain/serialize.ts`
must be empty against the step 2 state before committing.

- [ ] **Step 4: Every renderer still ignores the note**

Notes are invisible by spec, and `notes.ts` is the one stripper both
renderers import (registry #18). Run:

```bash
bun test tests/epub.test.ts tests/mobi.test.ts
```

Expected: PASS, and add one test to `tests/epub.test.ts` that renders a
`.fountain` containing an `[[img: ...]]` note and asserts the note's text
appears nowhere in the XHTML.

- [ ] **Step 5: Commit**

```bash
git add src/fountain/notes.ts src/fountain/serialize.ts tests/notes.test.ts tests/serialize.test.ts tests/epub.test.ts
git commit -m "Images: the [[img: ...]] note, written and read, and a speech a picture cannot split"
```

### Task 9: The picture folder, shared by every caller

**Files:**
- Create: `src/library-images.ts`
- Modify: `src/convert.ts`, `src/cli.ts`, `src/cli-errors.ts`, `desktop/ui/convert.js`
- Test: `tests/library-images.test.ts` (adapted from `tests/cli-sidecar.test.ts`), `tests/cli.test.ts`

- [ ] **Step 1: Port the folder module under its new name**

```bash
git show worktree-images:src/cli-sidecar.ts > src/library-images.ts
git show worktree-images:tests/cli-sidecar.test.ts > tests/library-images.test.ts
sed -i '' 's#../src/cli-sidecar#../src/library-images#' tests/library-images.test.ts
bun test tests/library-images.test.ts
```

Then adapt, keeping every guard (lstat refuses symlinks and FIFOs;
`MAX_SIDECAR_BYTES`; `isPng`; the 60 cap; prune deletes only names
`isGeneratedImageFile` accepts, plus its own temp files):

- Its private `writeFileAtomic` goes. Writes use today's
  `src/atomic-write.ts` (`writeFileAtomicSync`, `tempPathFor`). Check the
  prune predicate still recognises `tempPathFor`'s temp-name shape.
- The module name says what it is: rename `sidecarDir` to
  `imagesDirFor` and the rest to match, in code and tests together.

Expected after adapting: `bun test tests/library-images.test.ts` PASS.

- [ ] **Step 2: Carry pictures out of `convertPdf`**

```bash
git diff 658a2acf worktree-images -- src/convert.ts
```

Port ONLY: `BookImage`, the `images` field on `ConvertResult`, passing
`extractDocument`'s `images` to `parseLines`, and `imageWarnings`. Leave
out everything that hands images to `src/epub/` or `src/mobi/` (phase 2
and 3). `convert.ts` stays free of filesystem calls.

- [ ] **Step 3: The CLI writes the folder**

Today's `src/cli.ts` is about 1,000 lines larger than August's. Do not
apply the August diff; read it for intent:

```bash
git diff 658a2acf worktree-images -- src/cli.ts src/cli-errors.ts
```

Then, in today's conversion path, after the `.fountain` is written:
write the pictures with `library-images.ts` into the folder beside it
(the library's per-script folder or beside the input; one rule), prune
what the new conversion did not produce, and add `imagesPath` and
`imageCount` to the `--json` answer. `--no-fountain` writes no folder.
A folder that cannot be written is the new error code `unwritable`
(`src/cli-errors.ts`). Tests: port the phase-1 cases from
`git diff 658a2acf worktree-images -- tests/cli.test.ts` (those about the
folder, the JSON fields and `unwritable`; not the ones about EPUB
contents).

- [ ] **Step 4: The window can name the new error**

`desktop/ui/convert.js` has a `HEADINGS` map from engine error codes to
headings. Add `unwritable`, worded for a person ("Screepub could not save
the pictures from this script"), and extend whichever
`tests/desktop-ui.test.ts` test pins that every engine code has a
heading.

- [ ] **Step 5: Run everything that moved**

```bash
bun test tests/library-images.test.ts tests/cli.test.ts tests/convert.test.ts tests/desktop-ui.test.ts
bunx tsc --noEmit
```

Expected: PASS, clean.

- [ ] **Step 6: Commit**

```bash
git add src/library-images.ts tests/library-images.test.ts src/convert.ts src/cli.ts src/cli-errors.ts desktop/ui/convert.js tests/cli.test.ts tests/desktop-ui.test.ts
git commit -m "Images: the picture folder beside the .fountain, written by the CLI, read by anyone"
```

### Task 10: Proof

**Files:**
- Modify: `docs/formatting-options-log.md`, `tools/torture-manifest.json` only if a test demands it

- [ ] **Step 1: The full suite and the typecheck**

```bash
bun test
bunx tsc --noEmit
```

Expected: 0 fail, clean.

- [ ] **Step 1b: Windows, for real**

`tools/smoke-cli.ts` converts a fixture with a compiled binary; the cross
CI job runs it on `windows-latest`. Extend it to also convert
`tests/fixtures/jpx.pdf` and assert the `--json` answer's `imageCount` is
1, so every push proves the JPEG 2000 decoder loads on Windows (and
Linux). Its own tests (`tests/smoke-cli.test.ts`) take the new case.

- [ ] **Step 2: The corpus diff (CLAUDE.md: the tool for "does this matter")**

Convert every script in the root `/fixtures/` folder (gitignored, real
scripts; never commit any of it or quote a title) with `origin/main` and
with this branch, writing only the `.fountain`, and diff. Expected: the
only added lines are `[[img: ...]]` notes and the blank lines around
them; no other line changes. Record the count of notes per script (not
titles) in the commit message.

- [ ] **Step 3: The registry**

Add entry **#19 (pictures recognised, recorded as notes)** to
`docs/formatting-options-log.md`, in the house format of the entries
around it: what the behaviour is, the six rules and their constants, the
note syntax, the folder, the cap, and that the book is unchanged until
phase 2. Cite the August audit (commit `1337863`) for the thresholds.

- [ ] **Step 4: Commit**

```bash
git add docs/formatting-options-log.md
git commit -m "Images phase 1: registry #19, and the corpus diff adds notes and nothing else"
```

---

## Phases 2 to 4 (planned after phase 1 merges)

Each gets its own plan, written against the code phase 1 leaves:

- **Phase 2, Carry:** the `includeImages` option (default ON, owner) in
  `src/options.ts`, `format-defaults.json` and the window's Tune page;
  EPUB figures as `<p class="figure wNN">` with width classes in 5%
  steps; packaging only the pictures the book uses; the preview's CSP
  gains `img-src 'self' data:`; `src/export/artifact.ts` reads the
  folder through `library-images.ts`. When `includeImages` lands,
  `tests/serialize.test.ts`'s byte-stability test ("is byte-stable under a
  settings flip") regains its `includeImages: false` flip beside the
  `preserveFontShifts` one (the test's comment says where).
- **Phase 3, Kindle:** MOBI image records (port `92d232e`), and a device
  check of AZW3 and KFX with a figure.
- **Phase 4, Cover:** `promoteCoverImage` (default ON, owner), both cover
  declarations, the Tune toggle, and a device check of the library grid.
