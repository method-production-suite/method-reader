# Images in the book, phase 4 (Cover): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A designed title card (a full-page picture standing before the first scene heading) becomes the book's cover, so a library shows artwork instead of a grey rectangle. On by default (owner, 2026-10-01), with a Tune toggle.

**Architecture:** One picker, `pickCover` in `src/fountain/cover.ts` (the third module of the slug.ts/notes.ts kind: which picture is the book's face is not a rendering decision, and two copies would make two books disagree). The EPUB declares both cover spellings and skips the plate in the flow; the MOBI writes EXTH 201 and KEEPS the plate in the flow (August measured that Calibre's MOBI reader drops our cover record, so a MOBI that skipped it could lose the card entirely). Spec: `docs/superpowers/specs/2026-10-01-images-in-the-book-design.md` (phase 4, decision 2, the cover risk).

**Tech Stack:** Bun + TypeScript, JSZip, the PalmDB writer.

---

## Ground rules

- Branch `images-cover`, cut from `images-carry` (d5bd60e) so phases 2+3 can ship without this.
- August source on branch `worktree-images`: 30392ed (cover promotion in the EPUB, `promoteCoverImage`, both declarations, skip in flow but count as used, drop the emptied section) and the cover half of 92d232e (`src/fountain/cover.ts`, `pickCover`, `MobiArt.coverIndex`, EXTH 201, the plate stays drawn in the MOBI). Port the intent onto today's code; today's numbering lives in `mobiArt`, today's predicate is `rendersImage`.
- NEVER run Calibre or Kindle Previewer by hand; only inside bun test (isolated). No real screenplay names.

### Task 1: The picker, the option, and the EPUB cover

**Files:** `src/fountain/cover.ts` (new), `src/options.ts`, `format-defaults.json`, `src/epub/html.ts`, `src/epub/build.ts`, `desktop/ui/tune.js`, tests, the places a new option key must be pinned (the phase 2 `includeImages` commit 322d9e4 shows every one: tests/options, sidecar override row, cli-settings counts, settings-app-defaults, desktop-ui counts, tune.js/surfaces.css comment counts).

- [ ] `pickCover(tokens, format, have)`: the first picture note, before the first scene heading and with nothing else rendered before it (position, not page number), that `rendersImage` would render AND is `full`, when `format.promoteCoverImage` and `format.includeImages`; else undefined. Tests: a card before the first heading is picked; a full plate after a heading is not; a non-full picture before the heading is not; text before the card means no cover (decide and pin what counts as "nothing else": the title page block is not body content); off when either option is off; a hostile or missing name is not picked.
- [ ] `promoteCoverImage: true` in options.ts and format-defaults.json; Tune toggle in "What the book carries", label "Title card as the cover", plain help text, no em dash; its effect follows the source-grep classifier (if html.ts reads the key it is `'live'`; say which and why).
- [ ] EPUB: the cover picture is skipped in the flow but counted in `usedImages` (or the packager drops its bytes, then its manifest item, then the declarations, silently); the Opening section opens only for a note that renders IN THE FLOW (one predicate: renders and is not the cover), so a card-only front no longer opens an empty Opening; the package declares both cover spellings (`<meta name="cover" content="ID"/>` and `properties="cover-image"` on the manifest item); epubcheck clean. Tests: images.pdf and torture.pdf books have the cover declared, no Opening section, sc-001 is the first real scene; with promoteCoverImage off the phase 2 book is byte-identical (pin by comparing to the off-path); the preview follows the same body.
- [ ] Commit: "Images: a designed title card becomes the book's cover".

### Task 2: The MOBI cover, docs, device rows

**Files:** `src/mobi/html.ts`, `src/mobi/writer.ts`, `src/convert.ts`, registry, manifest, device checklist, tests

- [ ] `mobiArt` carries `coverIndex` from `pickCover`; buildMobi writes EXTH 201 (the 0-based record offset from the first image record, per 92d232e's measurement) and the plate stays in the flow (August: Calibre's MOBI reader looks the cover up by a path our file does not have, so a skipped plate would vanish). A cover index past the last picture throws. Test with the Calibre round trip inside bun test if 92d232e's test shape still fits.
- [ ] Registry: new #19b "The title card as the cover" (manifest row too, device-side: the cover in the library grid, which Amazon deletes for USB-sent non-Amazon books on newer Kindles; the cover inside the book; Kobo/Books). Update #19a's Opening wording.
- [ ] Corpus diff (anonymous) against images-carry: only the scripts with a front title card change, and only by the cover.
- [ ] Commit: "Images: the MOBI names its cover, and the plate stays on its page".
