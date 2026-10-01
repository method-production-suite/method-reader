# Images in the book, phase 3 (Kindle): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The engine's own MOBI (the Kindle file built when Calibre is absent) carries the same pictures as the EPUB, so no route to a Kindle loses them. A MOBI with no pictures stays byte-identical apart from its PDB timestamps.

**Architecture:** A picture in a MOBI 6 is an image RECORD after the text records, referenced from the markup as `<img recindex="0000N"/>` (1-based, five digits). The markup and the records are one numbering with two ends, so one function (`mobiArt`) owns the order and both the renderer and the writer read it. Header fields follow two real writers (Calibre and kindlegen), read back with `calibre-debug --inspect-mobi` in August. No cover (EXTH 201) in this phase: that is phase 4.

**Tech Stack:** Bun + TypeScript, the hand-built PalmDB writer in `src/mobi/writer.ts`.

---

## Ground rules

- Branch `images-carry`, on top of phase 2 (b6f049d). Phases 2 and 3 ship in ONE release, after the identifier release (owner, 2026-10-01).
- August source: branch `worktree-images`, commit 92d232e ("The MOBI grows image records, and addresses them by number"). Its message carries every measured header fact; read it in full. Port its MOBI half and leave its cover half (`src/fountain/cover.ts`, `pickCover`, EXTH 201, the promoted-cover drawing) for phase 4.
- Today's files moved since August: read for intent, write against today's code and tests.
- NEVER run Calibre, ebook-convert, calibre-debug or Kindle Previewer by hand. A test may run them only inside `bun test`, which isolates Calibre's config (tests/isolate-calibre-config.ts), passing `process.env`, and must skip with a reason when Calibre is absent.

### Task 1: Image records and the markup that names them

**Files:** `src/mobi/html.ts`, `src/mobi/writer.ts`, `tests/mobi.test.ts`

- [ ] `mobiArt(tokens, format, have)`: the ordered list of pictures the MOBI body shows, decided by the same `rendersImage(raw, format.includeImages, have)` predicate as the EPUB, deduped, first use wins. The ONE copy of the numbering.
- [ ] `tokensToMobiHtml` writes a figure as the MOBI dialect's equivalent of the EPUB figure, with `<img recindex="0000N"/>` from `mobiArt`'s order, no width attribute (the note's `w=` is a share of the column, which this dialect cannot state; August's reasoning). A full-page plate gets the dialect's page breaks around it if the renderer already uses `<mbp:pagebreak/>` for scene breaks; read today's html.ts and follow its existing break idiom. Never a page marker on a figure.
- [ ] `buildMobi({ ..., images })`: image records after the text records; header 108 (first image record, absolute), 194 (last content record extends past the pictures), 80 (first non-book record is the first non-text record, now an image), each with a test reading the field back from the built bytes. A book with no pictures: byte-identical to before (mask PDB dates; find where they sit).
- [ ] When Calibre is present, inside bun test: convert the built MOBI back with `ebook-convert` to EPUB in a scratch dir and assert the pictures come back byte-identical, in order, each referenced by the right `<img>` (August's end-to-end proof). Skip with a reason otherwise.
- [ ] Commit: "Images: the MOBI grows image records, and addresses them by number".

### Task 2: Every route hands the MOBI its pictures

**Files:** `src/convert.ts`, `src/export/artifact.ts`, `src/cli.ts` (help text), tests

- [ ] `renderBooks` passes the same filtered pictures to the MOBI as to the EPUB (one array; `mobiArt` decides which ride).
- [ ] The Kindle rebuild (`freshKindleArtifact`'s MOBI rung): the .mobi now carries pictures. Its rule that keeps the library EPUB when a picture is missing stays; the comment that says the .mobi "shows no picture yet" changes to what is true. Decide and say what a missing picture means for the .mobi handed over (built without it; the export answer still has no warnings channel; a follow-up item, not this phase).
- [ ] Stale text: the CLI help ("The .mobi does not show them yet"), registry #19 and #19a ("Not yet" bullets), `tools/device-checklist.ts` (the pictures section says the .mobi carries none), CLAUDE.md if it says so. Search for "mobi" near "picture" across src/, tools/, docs/formatting-options-log.md.
- [ ] Commit: "Images: the Kindle file built without Calibre carries the pictures too".

### Task 3: Proof

- [ ] Full `bun test`, `bunx tsc --noEmit`, epubcheck unchanged.
- [ ] Corpus diff (anonymous): every picture-less script's MOBI byte-identical to phase 2's apart from PDB dates; the picture scripts' MOBIs gain exactly their image records and figure markup; the Calibre round trip (inside bun test or a scratch test file run with bun test) returns every picture.
- [ ] Device checklist and torture manifest: row 19a gains the MOBI checks (sideload the engine's .mobi: pictures present, sizes as the device chooses, full-page plate on its own page).
- [ ] Whole-branch review of phase 3 with phase 2's seams.
