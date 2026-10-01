# Images in the book, phase 2 (Carry): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The pictures phase 1 recognised appear in the EPUB, the reader preview and every book the engine rebuilds, behind one option, `includeImages`, on by default, with a Tune toggle. A book with no pictures stays byte-identical.

**Architecture:** One predicate decides whether a note renders (option on, name allowlisted, picture present). The EPUB renderer places a figure through a no-marker path; a `src` resolver lets the same markup point at `../images/NAME` in the book and at a `data:` URI in the preview. The packager ships only the pictures the body used. Every caller that renders from a `.fountain` (the CLI's re-render, the Kindle export) reads the picture folder through `src/library-images.ts`. Spec: `docs/superpowers/specs/2026-10-01-images-in-the-book-design.md` (approved). Phase 1: `docs/superpowers/plans/2026-10-01-images-phase-1.md`.

**Tech Stack:** Bun + TypeScript, JSZip, fountain-js, the Tauri window's static UI (`desktop/ui`), epubcheck (brew-installed).

---

## Ground rules (same as phase 1)

- Branch `images-carry`, cut from `images` at a196b91 so phase 1 stays mergeable on its own. Ships after the identifier release, IN THE SAME RELEASE AS PHASE 3 (MOBI image records): the owner chose "the MOBI fallback carries pictures", and between phase 2 and 3 the Kindle fallback would carry an EPUB with pictures and a MOBI without.
- August source: branch `worktree-images` (BASE 658a2acf). Phase-2 commits: d35d911 and 6440c6c (figure markup and CSS), 8ae3794 (packaging and wiring), e0de8cd (the option). Phase 4 material (30392ed: `pickCover`, `promoteCoverImage`, cover meta, empty-section dropping) and phase 3 material (92d232e: MOBI records) are NOT ported here.
- Today's files have all moved since August; port by reading August for intent and writing against today's code, with today's tests.
- CSS stays CSS 2.1-vintage (no `min()`/`clamp()`/`var()`); horizontal in %, vertical in em; no `max-width` anywhere in the sheet, not even in a comment (a test greps the whole sheet); a break sits on a text-carrying element, never a structural wrapper (registry #8b).
- The worktree shell guard: plain single commands, scripts in the scratchpad. Never let a real script's title reach anything.

## File structure

| File | Change |
| --- | --- |
| `src/options.ts`, `format-defaults.json` | `includeImages: true` |
| `src/fountain/notes.ts` | `rendersImage(raw, format, have)`: the ONE predicate |
| `src/epub/html.ts` | `place` (no marker) beside `emit`; the figure; a note that renders opens the Opening; `usedImages`; `imageSrc` resolver |
| `src/epub/css.ts` | `.figure`, width classes `p.wNN`, `.figure-full`, only when the book has figures |
| `src/epub/build.ts` | picture entries (STORE), manifest items, collision-safe ids, only used pictures |
| `src/convert.ts` | pictures into both renderers' inputs; `convertFountain` accepts a picture map; a `pictures` progress stage |
| `src/cli.ts` | `.fountain` input reads the folder when `includeImages` is on; folder warnings |
| `src/export/artifact.ts` | the Kindle rebuild reads the folder |
| `src/export/route-*.ts` | email-size warning |
| `desktop/ui/tune.js`, `desktop/ui/convert.js`, `desktop/src-tauri/tauri.conf.json` | the toggle, the `pictures` stage, `img-src 'self' data:` |
| `tools/make-fixture.py`, `tests/fixtures/torture.pdf`, `tools/torture-manifest.json`, `tools/device-checklist.ts` | the picture sheets and their device checks |

---

### Task 1: The option and its toggle

**Files:** `src/options.ts`, `format-defaults.json`, `tests/options.test.ts`, `desktop/ui/tune.js`, `tests/desktop-ui.test.ts`, `CLAUDE.md`, `CONTRIBUTING.md`, `app/README-FROZEN.md` (only if `tests/frozen-app.test.ts` allows a non-Swift edit there; check first).

- [ ] Failing tests first: `tests/options.test.ts` gains `includeImages` in its defaults equality (`toEqual` against format-defaults.json, so both files move together) and a resolve test (`resolveFormatOptions({ includeImages: 0 })` gives `true`; `false` gives `false`). The desktop-ui parity test (keys equal `DEFAULT_FORMAT_OPTIONS` keys) fails until the toggle exists.
- [ ] `includeImages: true` in both files; one `bool()` resolve line.
- [ ] Tune: one toggle in the `front` group ("What the book carries"), label "Pictures from the script", help text in plain words with no em dash, saying pictures appear in the book and the preview, and that a book with no pictures is unchanged. Its effect: `'live'` if html.ts/css.ts read the option (they will after Task 2), so the source-grep effect classifier's counts move from `[13,1,4]` to `[14,1,4]`; update the literal 18s to 19 (tests/desktop-ui.test.ts ~3852, ~4204, ~7786, ~7826; "eighteen" in tune.js and surfaces.css comments).
- [ ] The frozen Swift app decodes format-defaults.json with synthesized `Decodable` and ignores unknown keys (app/Sources/KitCheck/main.swift ~202-215, read 2026-10-01): kit-check stays green, and the Swift app inherits `true` without a toggle (why this ships after F2). CLAUDE.md's "change all three together" for format-defaults.json becomes "options.ts and format-defaults.json together; kit-check ignores keys it does not know"; same sentence in CONTRIBUTING.md (~86-88).
- [ ] Commit: "Images: the includeImages option, on by default, and its Tune toggle".

### Task 2: The figure

**Files:** `src/fountain/notes.ts`, `src/epub/html.ts`, `tests/notes.test.ts`, `tests/epub.test.ts`, `tests/convert.test.ts`

- [ ] The predicate, test first, in notes.ts:

```ts
/** Whether an `[[img: ...]]` note becomes a figure: the option is on, the
 *  note parses (the allowlist is inside parseImgNote), and the picture is in
 *  the set the caller can actually ship. The ONE place this is decided, so
 *  the renderer, the section opener and (in phase 4) the cover agree. */
export function rendersImage(
  raw: string,
  format: { includeImages: boolean },
  have: ReadonlySet<string>,
): ImageRef | undefined
```

  Tests: off gives undefined; a hostile name gives undefined; a name not in `have` gives undefined; otherwise the parsed ref.
- [ ] html.ts: split `emit` into `place` (the block only) and `emit` (the block plus the pending page marker), as August did (d35d911); a figure goes through `place`, so it never takes a page marker, mirroring serialize.ts's `anchorsPageMarker`. The `note` case: `const img = rendersImage(raw, format, have)`; if set, place `<p class="figure wNN[ figure-full]"><img src="SRC" alt="Illustration"/></p>` where NN is `Math.min(100, Math.max(5, Math.round(img.widthPct / 5) * 5))` and SRC comes from `imageSrc(file)` (default `../images/${file}`). `alt` is not empty on purpose (empty declares the picture decorative).
- [ ] `tokensToBody(tokens, { format, images?: ReadonlySet<string>, imageSrc?: (file: string) => string })` returns `usedImages: Set<string>`. `images` defaults to empty: every existing caller renders exactly what it did.
- [ ] Phase 1's trap: `tokensToBody` skips a note while no section is open. Now: a note opens the Opening only when `rendersImage(...)` says it renders. Tests: (a) pictures OFF, a title-card note before the first heading: the body is byte-identical to the note-free text (phase 1's guarantee kept); (b) pictures ON with the picture present: an Opening section holds the figure, and the headings follow from sc-002; (c) ON but the picture missing from `have`: identical to (a). Rewrite tests/convert.test.ts ~200-216 (images.pdf's sections) to expect (b).
- [ ] Commit: "Images: the figure, decided by one predicate, placed without a page marker".

### Task 3: The figure's CSS

**Files:** `src/epub/css.ts`, `tests/epub.test.ts`

- [ ] `screenplayCss(format, { hasFigures })`: when `hasFigures` is false the sheet is byte-identical to today (test it). When true, after `p.centered` and before the title-page block: `.figure { margin: <elementSpacingEm>em 0; text-align: center; }`, `.figure img { height: auto; }`, one rule per width `p.w5 img { width: 5%; }` through `p.w100 img { width: 100%; }`, and `p.figure-full { page-break-before: always; page-break-after: always; }`. No keep on `.figure` (August measured it inert across 17 seam offsets; 6440c6c). No column spelling for `.figure-full` (the scene break has none either; the multicol shadow pins stay as they are).
- [ ] Update the CSS header comment ("Two mechanisms sit outside that list" becomes three) and keep every existing pin green: the avoid inventory (breaks set to `always` are not counted), the whole-sheet `max-width` grep, the fmt invariants.
- [ ] Commit: "Images: the figure's CSS, only in a book that has figures".

### Task 4: Packaging

**Files:** `src/epub/build.ts`, `tests/epub.test.ts` (or a new `tests/epub-images.test.ts`)

- [ ] `buildEpub(meta, body, format, images: { name: string; bytes: Uint8Array }[] = [])` ships only names in `body.usedImages`, deduped (a duplicate is OPF-074), as `OEBPS/images/NAME` with JSZip `compression: 'STORE'` (PNG is already deflated; August measured 0.3% saved for about 330 ms per 60 plates, and Tune re-renders on every move). Manifest items `href="images/NAME" media-type="image/png"`, no `properties`, never in the spine; ids `img-` plus the name reduced to `[A-Za-z0-9_-]`, bumped `-2`, `-3` against ids already taken (`a.b.png` and `a-b.png` otherwise collide: RSC-005 plus RSC-008, measured by August).
- [ ] Tests: an unreferenced picture is not shipped; a referenced one is, with its manifest item; the id collision case; and, when `epubcheck` is on PATH, a book built from tests/fixtures/images.pdf validates clean (skip with a reason otherwise; CI gets it in Task 11).
- [ ] Commit: "Images: the book carries the pictures it uses, and only those".

### Task 5: Wiring through convert and the CLI

**Files:** `src/convert.ts`, `src/cli.ts`, `tests/convert.test.ts`, `tests/cli.test.ts`

- [ ] `convertPdf`'s render step passes its own `images` (names and bytes) to both: the `have` set to `tokensToBody`, the bytes to `buildEpub`, and `hasFigures: usedImages.size > 0` to the CSS. The preview is built only when the caller asks for it (`--preview-html` or `--preview-inline`), so the export path never base64s pictures for nothing.
- [ ] `convertFountain(text, { ..., images?: Map<string, Uint8Array> })`: keep only entries where `isSafeImageFile(name) && isPng(bytes)` (a JPEG named .png is OPF-029 plus PKG-022), then render as above.
- [ ] CLI, `.fountain` input: when the resolved options say `includeImages`, read the folder beside the input with `readImages(imagesDirFor(input))`, pass the pictures, and add `imageFolderWarnings(read)` to the warnings. Never write or prune the folder on `.fountain` input. Tests: a `.fountain` with notes and its folder renders figures; with the option off, nothing is read (spy or a folder that would warn); a folder with a symlink or a non-PNG yields the warning, not a figure.
- [ ] Commit: "Images: every render from a .fountain finds its pictures".

### Task 6: The preview shows pictures

**Files:** `src/convert.ts` (or wherever the preview is built), `desktop/src-tauri/tauri.conf.json`, `tests/desktop-shell.test.ts`, `desktop/README.md`

- [ ] The preview passes `imageSrc: (f) => 'data:image/png;base64,' + base64(bytes)` and `hasFigures` to the same `tokensToBody` and CSS. One markup path.
- [ ] CSP: `"default-src 'self'; img-src 'self' data:"` and nothing else loosened. Add a desktop-shell test pinning the exact CSP string; fix desktop/README.md's statement of it.
- [ ] Measure before committing: the `--json` answer size and the wall time of a Tune-style re-render (`.fountain` input with `--preview-inline`) for tests/fixtures/images.pdf and, anonymously, for the private corpus's heaviest picture document. Report the numbers in the commit body. If the heaviest case exceeds about 1 s or 25 MB, stop and report: downscaling preview pictures is the fallback, and it is a design choice.
- [ ] Commit: "Images: the reader preview shows the pictures, and the window allows data: images only".

### Task 7: The Kindle rebuild keeps the pictures

**Files:** `src/export/artifact.ts`, `tests/export-artifact.test.ts` (or the file that tests `freshKindleArtifact`)

- [ ] `freshKindleArtifact`'s MOBI rung calls `convertFountain` and rewrites the library EPUB in place; today it passes no pictures, which would strip them from the EPUB. Read the folder through `readImages(imagesDirFor(fountainPath))` (only when `includeImages`) and pass the pictures. Test first: a library script with a figure, rebuilt through the MOBI rung, keeps its figure in the EPUB. The MOBI itself has no pictures until phase 3 (same release).
- [ ] Commit: "Images: rebuilding for a Kindle no longer strips the library book's pictures".

### Task 8: A progress share for pictures

**Files:** `src/parser/extract.ts`, `src/convert.ts`, `src/cli.ts`, `desktop/ui/convert.js`, `tests/cli.test.ts`, `tests/desktop-ui.test.ts`

- [ ] `decodeImages(onProgress?)` ticks per decoded page. `ConvertStage` gains `'pictures'`; shares become text 0-60, pictures 60-85, render 85-100 (a document with no pictures moves straight from 60 to 85). `nextProgress` and its label learn the stage ("Reading the pictures"); update the convert.js comment, tests/cli.test.ts's stage set (~611) and the desktop-ui progress tests (~1252-1325) together.
- [ ] Commit: "Images: reading the pictures has its own share of the progress bar".

### Task 9: The email size warning

**Files:** the email-to-Kindle route in `src/export/` (find it: `route-perform.ts` / `routes.ts`), its tests

- [ ] When the book being emailed is over 50 MB (Amazon's limit for email to Kindle), the route says so before opening Mail, in plain words with no em dash, and names the web uploader (200 MB) as the alternative. Test with a stubbed size.
- [ ] Commit: "Images: emailing a book too large for Kindle says so first".

### Task 10: The device sheets

**Files:** `tools/make-fixture.py`, `tests/fixtures/torture.pdf`, `tools/torture-manifest.json`, `tools/device-checklist.ts`, `tools/torture-content.py` (if it carries sheet text), `CLAUDE.md` (the "14 sheets" sentence)

- [ ] Port d2df8db's torture additions WITHOUT the cover expectation: the title card on sheet 0, the w=42 panel, the 1:2.75 w=56 tall plate, the letterhead on every sheet (furniture, refused). Row #19 becomes covered, with what each sheet pins. The device checklist gains a "pictures" section from d2df8db (minus cover): figure size, the full-page plate on its own page, the tall plate's slicing, the letterhead absent, dark mode on a transparent source.
- [ ] Commit: "Images: torture.pdf carries the pictures a device pass needs".

### Task 11: Proof

- [ ] Full `bun test`, `bunx tsc --noEmit`.
- [ ] CI: add `epubcheck` on books built from tests/fixtures/images.pdf and jpx.pdf beside the existing screenplay.pdf check (ci.yml ~81-84).
- [ ] The corpus diff (anonymous): every picture-less script's EPUB is byte-identical to phase 1 (identifier masked); the one script with a picture gains exactly an Opening with its figure and one image file; epubcheck clean for each.
- [ ] Registry: update #19 (or add #19a) in docs/formatting-options-log.md for the rendering: the predicate, the markup, the width classes, the CSS, packaging, the preview, the option, the device checks pending.
- [ ] Whole-branch review of phases 1 and 2 together.
- [ ] Not ours to do: the spec's device pass (a sideloaded KFX and AZW3 with a figure, from a freshly rebuilt torture.pdf) happens on the owner's hardware before this ships.

---

## Not in phase 2

The cover toggle: the spec's phase 2 lists "the two Tune toggles", but the second is the cover's and has nothing to switch until phase 4, so it ships there. Cover promotion (phase 4); MOBI image records (phase 3, same release); downscaling preview pictures (only if Task 6 measures a need); greyscale or size controls (spec: not in this design).
