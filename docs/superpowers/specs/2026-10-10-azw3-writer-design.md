# Design: Screepub writes AZW3 itself

Date: 2026-10-10 · Status: decided by the owner on 2026-10-10. Released as
beta before any device check, by the owner's choice.

## What

The engine writes AZW3 (KF8) from the EPUB it already builds, in TypeScript,
with no Calibre. The Kindle-over-USB ladder becomes:

1. **KFX**, when Calibre, the KFX Output plugin and Kindle Previewer are all
   present. Unchanged, and optional.
2. **Otherwise Screepub's own AZW3, always**, even where Calibre is
   installed. Calibre is now only for KFX.
3. **The engine's MOBI** stays as `--for mobi` and the Save a file row "for
   the oldest Kindles". The ladder no longer lands on it.

## Why

Kindle builds were the slowest step in the app because they started Calibre
(a Python start and a full conversion pipeline for every AZW3). The owner's
test Kindle (about 2021) reads AZW3 over USB. An AZW3 of our own is fast, is
the same on every computer, and needs nothing installed.

## Decisions

1. **TypeScript, in the engine** (`src/azw3/`). Not Rust: the window holds
   no logic.
2. **A port of Calibre's KF8 writer** (`calibre/src/calibre/ebooks/mobi/
   writer8/`: main, skeleton, index, exth, mobi, header, tbs, and the
   helpers it needs from `mobi/utils.py` and the PalmDOC compressor). Calibre
   is GPL-3; Screepub is AGPL-3.0-or-later, so the port is allowed. Credited
   in each file header and in THIRD-PARTY-NOTICES.md. Ported from the
   v9.11.0 tag, the Calibre this project tests against.
3. **The input is the EPUB file**, read back out of its zip: the same bytes
   every other route sends. So `export --for azw3` works from any library
   book, and the AZW3 can never disagree with its EPUB.
4. **KF8 only.** No joint MOBI 6 + KF8 file: a Kindle too old for KF8 takes
   the engine's MOBI, by name.
5. **Reuse `src/mobi/`**: the PalmDB container and the EXTH record shape move
   to one shared module, and the MOBI's bytes stay what they were.
6. **What carries over**: every spine document, chunked into skeletons and
   fragments as Calibre does; the stylesheet verbatim as a CSS flow (keeps,
   margins and all, registry #8b, #2); the title page; scene sections; the
   scene TOC as the NCX index; the landmarks as the guide (start reading at
   the body); title, author, language and identifier in EXTH; every picture
   as a resource record with the cover named in EXTH 201 (registry #19a,
   #19b).
7. **What does not**: no generated inline Table of Contents page (Calibre
   adds one); pictures stay PNG (Calibre re-encodes to JPEG); no cover
   thumbnail record (EXTH 202); CSS is not rewritten (Calibre flattens it).
   Each is listed in the registry entry with why.
8. **PalmDOC compression**, as Calibre and kindlegen both ship.
9. **Beta everywhere a reader meets it**: the window's Beta badge on the
   AZW3 save row, "(beta)" in the CLI help, the README, the site and the
   docs.

## Verification, without a device

- A pure-TypeScript structural reader in `tests/helpers/` that walks the
  PDB, the MOBI and KF8 header, EXTH, FDST, every INDX, the trailing bytes
  and the record offsets, and rebuilds the XHTML from skeletons and
  fragments. Runs in CI.
- Calibre's own reader, only inside `bun test`, isolated by
  `tests/isolate-calibre-config.ts`: `ebook-convert x.azw3 y.epub` and
  `calibre-debug --inspect-mobi`, checked for text, order, TOC, metadata,
  pictures and cover. Skips when Calibre is absent.
- A corpus test over root `/fixtures/` (runs only when it exists): every
  script's AZW3 must parse in Calibre's reader. Counts only.
- A structural comparison with Calibre's own AZW3 of the same EPUB, and the
  build times of both.

## What only a Kindle can answer

Whether the device opens and indexes the file, shows the cover in the
library, shows PNG pictures, honors the TOC and the start-reading position,
and how the KF8 renderer lays out our CSS. Registry #8b already records that
the KF8 renderer honors no keeps; that is the format, not the writer.
