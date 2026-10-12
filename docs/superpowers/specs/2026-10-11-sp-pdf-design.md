# sp-pdf: pdf.js's answers in pure Rust

Date: 2026-10-11. Part of the pure-Rust port
(docs/superpowers/plans/2026-10-11-pure-rust-port.md). Status: built, at
parity on every committed fixture and every local real script.

## What the parser reads from pdf.js

Read out of src/parser/extract.ts, images.ts, artwork.ts, png.ts,
first-page.ts and the pdf.js shims, against pdfjs-dist 6.3.289 as the engine
opens a document (`getDocument({ data, wasmUrl })`: no cMapUrl, no
standardFontDataUrl, no system fonts, `ignoreErrors` on, no OffscreenCanvas,
no ImageDecoder).

- **Pages.** `pdf.numPages`; per page `getViewport({ scale: 1 })` width and
  height (the view box: CropBox clipped to MediaBox, Letter by default, with
  /Rotate swapping the two). Nothing else of the page dictionary; no
  document metadata.
- **Text items** (`getTextContent()`, default options: normalization on, no
  marked content, whitespace not kept). Fields read: `str`, `transform` (all
  six: `[3]` is the font size, `[4]`/`[5]` the origin), `width`, `fontName`.
  `height`, `dir` and `hasEOL` are not read by extract.ts but are carried.
  Empty items (`str: ""`, the end-of-line markers) are dropped by the
  grouping but seen by the underline and repair passes, so they are kept.
  pdf.js's own grouping of glyphs into items decides the parser's words:
  where it inserts a space (`addFakeSpaces`, the 0.102-0.6 em window), where
  it splits an item (a jump left of -0.2 em, a vertical shift past a quarter
  of the height), and where it emits a whitespace item spanning a gap.
- **Font style.** After `getOperatorList`, `commonObjs.get(fontName).name`:
  the PostScript name the bold/italic/family regexes read. An unusable font
  (pdf.js's ErrorFont) has no name.
- **Operators** (`getOperatorList()`, display intent, forms enabled):
  `save`, `restore`, `transform` (the CTM walk), `constructPath` with its
  paint operator and Float32 bounding box (underlines), `paintFormXObject
  Begin/End` with the Float32 /Matrix (art placed in forms),
  `begin/endAnnotation` (stamps skipped), `paintImageXObject` with its object
  id and declared /Width and /Height (pictures), and `showText` glyph runs as
  joined Unicode (phantom-space repair). Inline images and stencil masks are
  never read.
- **Pixels.** Per painted id, pdf.js's `{ width, height, kind, data }`
  (kind 1 packed gray, 2 RGB, 3 RGBA), exactly as `PDFImage.createImageData
  (false, false)` produces it: JPEG through pdf.js's own decoder, JPEG 2000
  through OpenJPEG (its JS fallback build), JBIG2/CCITT through pdf.js's
  decoders, soft masks and colour-key masks folded into RGBA, ICCBased spaces
  falling back to /Alternate or the component count.

## The library choice

Evaluated from current crates.io sources (`cargo fetch` of each, read in
`~/.cargo/registry/src`), 2026-10-11:

| Crate | Version | License | Pure Rust | What it covers | Verdict |
|---|---|---|---|---|---|
| hayro-syntax | 0.8.0 | Apache-2.0 OR MIT | yes (`unsafe` only behind its opt-in `unsafe` feature, off here) | xref tables and streams, repair of broken xrefs, object streams, decryption, all standard filters, plus JPX/JBIG2/CCITT through hayro-jpeg2000 0.4.1, hayro-jbig2 0.3.1, hayro-ccitt 0.4.0; lazy, borrowed parsing; its parser's leniency is modelled on pdf.js (it cites pdf.js bug numbers in its number and content parsing) | **chosen, for the file layer** |
| hayro-interpret | 0.8.0 | Apache-2.0 OR MIT | yes | a renderer's interpreter: fonts (hayro-font, skrifa), glyph outlines, colour | not used: its text semantics are a renderer's, not getTextContent's |
| lopdf | 0.45.0 | MIT | yes | eager object model, nom content parser, editing; no JPEG 2000 or JBIG2 decoder; a large dependency tree (image, chrono, rayon, tokio optional) | rejected: missing decoders, whole-stream content parse failures |
| pdf (pdf-rs) | 0.10.0 | MIT | yes | typed object model (derive macros), jpeg-decoder, fax; no JPEG 2000 | rejected: missing JPX, slower maintenance |
| pdf-extract | 0.12.1 | MIT | yes | text extraction on lopdf | rejected: its own text heuristics, nothing like pdf.js's items |

No candidate reproduces pdf.js's text items, and none could: the item
boundaries, inserted spaces and widths are pdf.js's own algorithms, which
the parser's thresholds were tuned against. So the design is **hayro-syntax
for the file, and a faithful port of pdf.js for everything the parser can
see**: the content lexer and operator preprocessor (argument repair, the
33-operand limit, inline-image end detection), font loading (encodings,
Differences, ToUnicode CMaps, standard-14 metrics, the per-font quirks of
`extractDataStructures`, `buildToUnicode`, `fallbackToSystemFont`,
`charToGlyph`), getTextContent with its exact arithmetic, the operator-list
subset with pdf.js's Float32 rounding and image ids, `PDFImage`, the colour
spaces, and pdf.js's JPEG decoder (an IDCT and colour conversion that round
differently would put many JPEG pixels a level off). pdf.js's data tables
(glyph list, encodings, metrics, standard-font maps, the Unicode categories
its regexes test, the NFKC values its normalizer produces) are generated
from the pdfjs-dist build itself by `oracle/gen-tables.ts`, so they cannot
drift from what the engine ran on. pdf.js is Apache-2.0, compatible with
AGPL-3.0-or-later; the crate's NOTICE carries it and each ported module says
so.

Robustness: hayro-syntax returns `Option`/`Result` throughout; the one
panic found (its `Dict::entries` unwraps a re-read a damaged file can fail)
is avoided by reading keys one by one. `examples/mutate.rs` reads 400
deterministic corruptions of each committed fixture and 40 of each real
script (truncations, bit flips, spliced garbage, scrambled bytes): no panics.
`sp_pdf::read` still wraps each page and each image in `catch_unwind`, a
boundary for the library's code, not ours; a page that panicked would come
back with no text and no operator list, as pdf.js reports a page it cannot
read.

## Interface

`sp_pdf::read(bytes) -> Result<sp_parser::facts::DocumentFacts, LoadError>`:
the oracle's `pages.json` shape (`numPages`; per page `pageNum`, viewport
`width`/`height`, the untouched text items, `fonts` id to name or null,
`ops` as `facts::Op`, `images` id to `{ width, height, kind }` with
`dataLength`, `dataSha256` and the pixels in `ImageFact::data`).
Lower level: `Document::load`, `Document::reader()`, `Reader::page(i)` (text
items, font names, operators) and `Reader::image(objId)` (pixels on demand).

## Parity

`tests/facts.rs` reads each PDF itself and compares, stage by stage, with the
oracle dumps in rust/oracle/ (committed) and rust/oracle-real/ (local):

1. `read(pdf)` serialized equals `pages.json` exactly (every double bit for
   bit, compared with serde_json's `float_roundtrip`), with one mask: pdf.js
   numbers documents per process (`g_d0_` in one run, `g_d2_` in the next),
   so the document number inside a font or picture id is masked on both
   sides. Every picture's bytes equal `pixels/<dataSha256>.bin`.
2. Through sp-parser: `lines.json` (lines, page count, phantom-space
   repairs, picture placements, picture PNGs by decoded pixels, picture
   statistics).
3. `elements.json` (elements, characters, scenes, page count).

Results, 2026-10-11:

- **Committed fixtures (7):** 1,336 text items, 1,219 operators and 32
  picture paints (Flate RGB and lossless JPEG 2000): all identical,
  every position bit-exact; lines.json and elements.json identical for all
  seven.
- **Real scripts (7 PDFs, counts only):** 23,620 text items, 131,875
  operators, 186 picture paints (DCT with soft masks, DCT in ICCBased RGB,
  Flate RGB): all identical, bit-exact; lines.json and elements.json
  identical (see risk 1 for the one timing-dependent exception).

Tolerance: none is needed on this machine. The one platform-dependent step
is `Math.hypot`, which Bun and Rust both hand to the platform libm (checked
equal on 200,000 random pairs here); on axis-aligned text, which is all a
screenplay has, it is exact everywhere.

## Differences that remain (none reached by any fixture or real script)

Each is a code path pdf.js has and sp-pdf does not reproduce exactly. They
are listed so that a future file which reaches one is recognized.

1. **Embedded Type 1 and CFF font programs are not parsed.** pdf.js reads
   them for three things text extraction sees: the built-in encoding
   (`adjustType1ToUnicode`, which fills Unicode for codes the PDF's own
   encoding leaves out when there is no ToUnicode map), the program's
   FontMatrix (which rescales widths when it is not 1/1000: same widths up
   to the last bit of a double), and charstring widths for codes /Widths
   omits. A parse failure in pdf.js also marks the font missing, which
   changes a Type 1 font's "" glyphs to spaces. Affects a Type 1/CFF font
   with no ToUnicode and an incomplete /Encoding. Every real script here
   uses TrueType or the standard 14.
2. **TrueType repair is reduced to its two text effects**: whether
   `checkAndRepair` would throw (a missing head, hhea, maxp or loca), and the
   name records `adjustTrueTypeToUnicode` reads. TrueType collections are
   taken as loading. Deeper repair failures pdf.js can hit (bad loca it cannot
   fix, TTC without the named font) are not modelled.
3. **Type 3 fonts:** char procs are not evaluated, so a font whose char
   procs would fail to load (pdf.js's ErrorFont) and the `isCharBBox`
   adjustment of the text matrix for small Type 3 sizes are not reproduced.
4. **Built-in CMaps:** reproduced as unavailable (the engine passes no
   cMapUrl), which is pdf.js's behaviour: such fonts fail. No difference;
   listed because fixing it on one side only would create one.
5. **Colour spaces in images:** CalRGB, Separation and DeviceN (which need
   PDF functions) are not ported; such an image fails to decode (the engine
   counts it under `imageStats.failed`) where pdf.js would decode it. CalGray,
   Lab, Indexed, ICCBased, Gray, RGB and CMYK are ported.
6. **JPEG 2000** decodes through hayro-jpeg2000, not OpenJPEG. Lossless
   images match exactly (the committed fixture). A lossy (9/7) image may
   differ by a level where the two decoders' floating point differs; images
   above 8 bits per component are scaled by shifting, which is assumed, not
   checked, to match pdf.js's OpenJPEG glue; an sYCC JP2 is converted to
   RGB by hayro where the glue's behaviour is unchecked. The reduce-power
   rule pdf.js applies to JPX larger than 2048 pixels (no canvas under Bun)
   is ported.
7. **Annotations:** an annotation's own /AP appearance is walked with
   pdf.js's visibility flags and its widget rule (forms enabled: only
   signature widgets draw). Appearances pdf.js synthesizes for annotations
   that lack one (Square, Circle, Line, Ink, Highlight, FreeText and others)
   are not generated.
8. **Strings above 255 in content streams:** a three-digit octal escape
   above \377 is a char code above 255 in pdf.js and a byte here. A lone
   UTF-16 surrogate from a broken ToUnicode becomes U+FFFD (Rust strings
   cannot hold one).
9. **pdf.js image-id prefix ceiling:** pdf.js stops promoting repeated
   images to document-wide `g_d0_` ids once it holds ten of them and 50 MB;
   that ceiling is not modelled. It changes an id's spelling, never which
   pixels an id names.
10. **Page tree:** pages are hayro-syntax's walk of /Kids; pdf.js trusts
    /Count first. They differ only on a malformed page tree.

## Risks for the parser port

1. **The picture budget is wall-clock.** sp-parser charges its 20-second
   IMAGE_BUDGET_MS to the work of encoding pictures, as the TypeScript does.
   On the 30-page, 172-paint real deck, the Rust encode runs close enough to
   the ceiling that under load some runs exhaust it and report
   `imageStats.failed > 0` (lines.json then differs). The difference is in
   sp-parser's encoder speed (miniz_oxide at level 9), not in sp-pdf's
   facts, and it makes that comparison timing-dependent. tests/facts.rs
   reports such a run by name ("picture budget spent") after its facts and
   pixels have matched, rather than failing on the machine's speed; the
   committed fixtures, far inside the budget, get no such exception. The
   same deck matched at every stage on an unloaded run.
2. **Eager pixels.** `read` decodes every painted picture up front. The
   TypeScript reads text first and decodes pictures only after the
   scanned/not-a-screenplay guards pass, so a refused document never pays
   for its pictures, and the budget counts decoding as well as encoding. For
   parity of cost and of the budget, the engine should use `Reader` (text
   now, `image()` later) rather than `read`; the facts API should grow a
   lazy picture accessor before sp-engine wires it.
3. **The oracle's ids carry pdf.js's per-process document number.** Anything
   comparing ids against pages.json has to mask `g_dN_`, as tests/facts.rs
   does.
4. **JSON numbers.** The oracle's doubles must be read with serde_json's
   `float_roundtrip` (now on in the workspace) or comparisons land one ulp
   off.
