# Third-party notices

Method Reader's own code is licensed under the MIT License or the Apache
License 2.0, at your option: see [`LICENSE-MIT`](LICENSE-MIT) and
[`LICENSE-APACHE`](LICENSE-APACHE). The one exception is the AZW3 writer,
`rust/crates/sp-azw3`, which is ported from calibre and so is licensed under
the GNU General Public License version 3
([`LICENSE-GPL-3.0`](rust/crates/sp-azw3/LICENSE-GPL-3.0)). The command line
and the app both contain that writer, so each program as a whole is
distributed under GPL-3.0; every other crate can be used on its own under
MIT or Apache-2.0. All three licence files travel inside each app bundle.

## What ships, and what it carries

Method Reader is written in Rust, as one Cargo workspace (`rust/`). Two programs
are built from it for download, and everything third-party that either one
carries is listed in this file.

- **The command line**, `method-reader` (crate `sp-cli`), ships as the
  command-line archives for macOS (Apple silicon), Linux (x64 and arm64) and
  Windows (x64). An archive holds only the binary; this file is its notice.
  The binary carries the conversion engine (Method Reader's own crates, including
  the ported code credited below) and the third-party Rust crates marked
  "CLI and app" in [the crate tables](#the-rust-crates-that-ship).
- **The window**, `method-reader-app` (crate `sp-app`, built as
  `method-reader-app`), ships as the app: the Mac `.dmg`, the Windows
  installer, and the Linux `.deb`, `.rpm` and AppImage. It has the whole
  engine built in, so it carries everything the command line does, plus the
  updater (crate `sp-update`), the window's own crates (egui and eframe,
  blitz and stylo for the preview, rustls and ring for HTTPS, and the rest
  marked "app" in the tables) and the two typefaces below. This file travels
  inside each bundle beside `LICENSE`.
- **The device-request service** (crate `sp-device-requests`) is a
  Cloudflare Worker that Method Reader runs for itself. It is not in any download,
  so its crates are not listed here.
- **Developer tools** (`xtask`, `sp-release`) and the beta MTP helper
  (`sp-mtp`, `method-reader-mtp`, not yet shipped) are not in any download either.

The installers wrap the window in other people's packaging programs, which
are not part of Method Reader: the Windows installer is made with NSIS, and the
AppImage starts with AppImage's type2-runtime, the small program that mounts
it. Each is distributed under its own licence; see
<https://nsis.sourceforge.io/License> and
<https://github.com/AppImage/type2-runtime>.

---

## Code ported from other projects

These projects' libraries do not ship. Parts of their code were translated
into Method Reader's Rust crates, so their licences travel with that code. Each
ported file names what it ports at its head.

### pdf.js (pdfjs-dist 6.3.289): Apache License 2.0

PDF text extraction. Copyright Mozilla Foundation and pdf.js contributors.
<https://github.com/mozilla/pdf.js>

`rust/crates/sp-pdf` reproduces what pdf.js hands Method Reader's parser: the
text items of `getTextContent`, the fonts behind them, the operators the
parser walks out of `getOperatorList`, and each painted image's pixels. Its
modules are translations of pdf.js's `src/core/` code: the content-stream
lexer and parser (`parser.js`), the evaluator's text grouping, operator list,
argument repair and font loading (`evaluator.js`), font bookkeeping and the
TrueType `name` table handling (`fonts.js`), CMaps (`cmap.js`), the bidi pass
(`bidi.js`), glyph names and Unicode helpers (`unicode.js`, `glyphlist.js`,
`standard_fonts.js`, `encodings.js`, `src/shared/util.js`), image decoding
(`image.js`, `colorspace.js`, `image_resizer.js`, `jpx.js`), the JPEG
decoder (`jpg.js`, itself derived from Mozilla's jpgjs project), the stream
filters (`flate_stream.js`, `predictor_stream.js`, `lzw_stream.js`,
`ascii_hex_stream.js`, `ascii_85_stream.js`, `run_length_stream.js`), and
page geometry (`document.js`, `annotation.js`, and
`src/display/display_utils.js`'s `PageViewport`). The data tables in
`rust/crates/sp-pdf/src/tables.rs` were generated from pdfjs-dist 6.3.289.
The Adobe Glyph List and the standard 14 fonts' metrics in those tables
reach pdf.js from Adobe's published glyph list and AFM files, which Adobe
licenses for this use (BSD-style licence, Copyright Adobe Systems). The
crate's own `NOTICE` file carries the same notice.

Licensed under the Apache License, Version 2.0 (the "License"); you may
not use this file except in compliance with the License. You may obtain a
copy of the License at <http://www.apache.org/licenses/LICENSE-2.0>.

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
License for the specific language governing permissions and limitations
under the License.

The full Apache License 2.0 text is under [Licence texts](#licence-texts).

### fountain-js 1.2.4: MIT

Fountain tokenizing. Copyright (c) 2020 Jonny Greenwald, Matt Daly.
<https://github.com/jonnygreenwald/fountain-js>

`rust/crates/sp-fountain/src/tokenizer.rs` is a line-for-line port of
fountain-js's `dist/lexer.js`, `token.js` and `rules.js`, with the rules
copied verbatim.

Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the "Software"),
to deal in the Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, sublicense,
and/or sell copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

### JSZip 3.10.1's zip container: MIT (dual-licensed MIT or GPL-3.0)

Copyright (c) 2009-2016 Stuart Knightley, David Duponchel, Franz Buchinger,
António Afonso. <https://stuk.github.io/jszip/>

`rust/crates/sp-epub/src/zip.rs` writes the EPUB's zip container the way
JSZip 3.10.1 writes it (`lib/generate/ZipFileWorker.js`), field for field:
every header field (versions, flags, method, dates, CRC, sizes,
attributes) is JSZip's, and so is the folder entry it puts ahead of the
first file in each folder. The compression itself is
not JSZip's or pako's: it is the zlib-rs crate, listed below. JSZip is dual
licensed; Method Reader uses it under the MIT license, whose text is the same as
fountain-js's above.

### Node.js's `path` module: MIT

Copyright Joyent, Inc. and other Node contributors.
<https://github.com/nodejs/node>

`rust/crates/sp-fsutil/src/nodepath.rs` is a line-for-line port of Node's
`path` functions (`lib/path.js`), posix and win32, so the paths Method Reader
prints are the ones its earlier engine printed. Node's `path` is under the
MIT license, whose text is the same as fountain-js's above.

### calibre 9.11.0, ported in part: GPL-3.0

Copyright Kovid Goyal and the calibre contributors.
<https://github.com/kovidgoyal/calibre>

Method Reader's AZW3 (KF8) writer, `rust/crates/sp-azw3/src/`, is a Rust port of
parts of calibre's KF8 writer, `src/calibre/ebooks/mobi/writer8/`: main.py
(the text records, FDST, indices, guide, and the markup passes),
skeleton.py (the chunker), index.py, tbs.py, mobi.py (record 0 and the
record layout) and exth.py; the helpers it takes from
`src/calibre/ebooks/mobi/utils.py` (encint, to_base, CNCX, align_block,
utf8_text, encode_tbs, encode_fvwi, encode_trailing_data);
writer2/resources.py's resource records; and the PalmDOC compressor,
cpalmdoc_do_compress in `src/calibre/ebooks/compression/palmdoc.c`. The
test suite's KF8 reader (`rust/crates/sp-azw3/tests/common/kf8_reader.rs`)
ports parts of calibre's reader (`src/calibre/ebooks/mobi/reader/index.py`,
`reader/mobi8.py`, and utils.py's decint and get_trailing_data), and its
PalmDOC decoder (`tests/common/palmdoc_decode.rs`) is a port of
cpalmdoc_decompress; neither test file is in any download.

calibre is licensed under the GNU General Public License, version 3, and
so is the port: sp-azw3 is GPL-3.0-only (full text in
[`rust/crates/sp-azw3/LICENSE-GPL-3.0`](rust/crates/sp-azw3/LICENSE-GPL-3.0)).
The rest of Method Reader is MIT OR Apache-2.0, which GPL-3.0 code may be
combined with; the programs that contain sp-azw3 are distributed under
GPL-3.0 as a whole.

The script `rust/crates/sp-engine/src/export/kfx_install.py`, which
`method-reader kfx-install` hands to your own calibre's `calibre-debug`, is
Method Reader's own: it calls calibre's functions inside your calibre and copies
none of calibre's code.

---

## Typefaces

### Courier Prime 1.203: SIL Open Font License 1.1

Copyright (c) 2015 Alan Dague-Greene, Quote-Unquote Apps.
<https://quoteunquoteapps.com/courierprime/>

Bundled as WOFF2 subsets in `site/fonts/` and in `rust/crates/sp-app/fonts/`,
built into the window, so neither the website nor the window makes a
third-party request. Not shipped inside any converted book: the EPUB and
MOBI ask the device for Courier Prime by name and fall back to whatever
typewriter face it carries.

### Literata 3.103: SIL Open Font License 1.1

Copyright (c) 2017 Type Together, commissioned by Google.
<https://github.com/googlefonts/literata>

Bundled as WOFF2 subsets in `site/fonts/` and in `rust/crates/sp-app/fonts/`,
for the script's prose in the window (the book preview).

### Inter 4.1 (font version 4.001): SIL Open Font License 1.1

Copyright (c) 2016 The Inter Project Authors.
<https://github.com/rsms/inter>

Bundled as two unmodified TrueType files (Regular and SemiBold) in
`rust/crates/sp-app/fonts-ui/`, with the licence beside them as
`fonts-ui/OFL.txt`. The window sets its controls, labels and messages in
the computer's own interface typeface, read from the system at start (San
Francisco on a Mac, Segoe UI on Windows, the usual sans on Linux); none of
those is shipped. Inter is the stand-in where no system face loads, and the
fallback for any letter the system face lacks.

The OFL requires that the fonts be redistributed under the same licence, that
this notice travel with them, and that they not be sold on their own. It also
forbids using the reserved font names on a modified version. The files here
are unmodified upstream builds. The full licence text is under
[Licence texts](#sil-open-font-license-11).

The window also carries the default fonts of its toolkit, egui (Hack, Noto
Emoji, Ubuntu Light and an emoji icon font); see
[The fonts inside egui](#the-fonts-inside-egui).

---

## Not bundled

[Calibre](https://calibre-ebook.com) (GPL-3.0) and Amazon's Kindle
Previewer are optional and used only if you installed them yourself:
Method Reader runs your copy of `ebook-convert`, with the KFX plugin, to produce
KFX. The AZW3 is Method Reader's own (see the calibre port above), so no Calibre
program and no Amazon code is distributed with Method Reader.

The **KFX Output plugin for Calibre** (GPL-3.0, copyright John Howell) is not
distributed either. `method-reader kfx-install`, or the Install button on the Send
page, downloads it from Calibre's own plugin index and has **your own copy of
Calibre** install it, only when you ask, where it runs as part of Calibre.
The plugin drives Amazon's Kindle Previewer (proprietary, installed by you)
to perform the actual KFX conversion. Releases of the older Mac app, which is
no longer published, carried an unmodified 2.12.0 copy of the plugin (with a
Traditional-Chinese rendering fix by lcandy2), aggregated rather than linked,
with the full GPL-3 text and their own copy of this notice beside it.

---

## The Rust crates that ship

Every third-party crate built into `method-reader` or `method-reader-app`, on any
platform: 453 crates (62 of them in the command line; all of them
in the window, which contains the command line's engine). The list comes from
Cargo itself, at the versions `rust/Cargo.lock` pins: `cargo tree --locked -p
sp-cli` and `-p sp-app`, each with `-e normal,no-proc-macro`, for each
shipped target (`aarch64-apple-darwin`, `x86_64-pc-windows-msvc`,
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`), with each crate's
version, licence and authors read from `cargo metadata --format-version 1
--locked --filter-platform` for the same targets. Development and test
dependencies are left out. So are the 34 procedural-macro crates (and
what only they use) and every build script and build-only dependency: they
run while compiling and are not part of the program that ships.

"Platforms" says where a crate is built in: "all", or the systems named
(macOS is Apple silicon only; on Linux the command line is built for x64
and arm64, the window for x64). Each download carries only its own
platform's share.

**Which licence Method Reader relies on.** Many crates offer a choice ("MIT OR
Apache-2.0"). Where MIT is one of the choices, Method Reader uses the crate under
MIT; otherwise under Apache-2.0 where offered. So the Unlicense, 0BSD,
LLVM-exception, GPL-2.0, MPL-2.0 and zlib alternatives some crates also offer
are not the ones relied on, and their texts are not repeated here. Where a
crate's licence is a conjunction ("AND"), every part applies. The tables are
grouped by the licence relied on, and each row also gives the licence
expression exactly as the crate's Cargo.toml declares it.

**Copyright.** The copyright lines are quoted from each crate's own licence
files in its published source (`~/.cargo/registry/src/.../<crate>-<version>/`,
or the git checkout for the blitz and taffy crates). Where a crate's
published source has no licence file, or its licence file names no holder,
the row says "copyright its authors, see the crate" and gives the authors
its Cargo.toml names.

**Git-sourced crates.** blitz-dom, blitz-html, blitz-paint, blitz-traits,
stylo_taffy and debug_timer come from <https://github.com/DioxusLabs/blitz>
at revision 45496d9288b063719ebeae4e0761273f7881b3d7 (the `rev =
"45496d9288b0"` in `rust/crates/sp-app/Cargo.toml`), and taffy from
<https://github.com/DioxusLabs/taffy> at revision
7d33901cc383489d9945ae796a1ceed0a935ef66, which the blitz crates name. Their rows say so.

### MIT (386)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| accesskit | 0.24.1 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: The AccessKit contributors) |
| accesskit | 0.25.1 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: The AccessKit contributors) |
| accesskit_atspi_common | 0.18.1 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: The AccessKit contributors) |
| accesskit_consumer | 0.35.0 | MIT OR Apache-2.0 | app | Windows | copyright its authors, see the crate (authors: The AccessKit contributors) |
| accesskit_consumer | 0.36.0 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: The AccessKit contributors) |
| accesskit_consumer | 0.38.0 | MIT OR Apache-2.0 | app | macOS | copyright its authors, see the crate (authors: The AccessKit contributors) |
| accesskit_macos | 0.26.3 | MIT OR Apache-2.0 | app | macOS | copyright its authors, see the crate (authors: The AccessKit contributors) |
| accesskit_unix | 0.21.1 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: The AccessKit contributors) |
| accesskit_windows | 0.32.1 | MIT OR Apache-2.0 | app | Windows | copyright its authors, see the crate (authors: The AccessKit contributors) |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | CLI and app | all | Copyright (C) Jonas Schievink <jonasschievink@gmail.com> |
| ahash | 0.8.12 | MIT OR Apache-2.0 | app | all | Copyright (c) 2018 Tom Kaitchuck |
| aho-corasick | 1.1.5 | Unlicense OR MIT | CLI and app | all | Copyright (c) 2015 Andrew Gallant |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | app | Linux, Windows | copyright its authors, see the crate (authors: Zakarum) |
| alsa | 0.11.0 | Apache-2.0/MIT | app | Linux | Copyright (c) 2015-2021 David Henningsson, and other contributors. |
| alsa-sys | 0.4.0 | MIT | app | Linux | Copyright (c) 2018 diwic |
| anyrender | 0.14.0 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| anyrender_svg | 0.15.0 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| anyrender_vello_cpu | 0.18.0 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| arboard | 3.6.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2022 The Arboard contributors |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | app | all | Copyright (c) Ulrik Sverdrup "bluss" 2015-2023 |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (its Cargo.toml names no authors) |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | app | Linux, Windows | Copyright (c) 2016 ASH |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2020 Yoshua Wuyts |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina) |
| async-executor | 1.14.0 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina, John Nunley) |
| async-io | 2.6.0 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina) |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina) |
| async-process | 2.5.0 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina) |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: John Nunley) |
| async-task | 4.7.1 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina) |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | app | Linux | Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors |
| atomic_refcell | 0.1.14 | Apache-2.0 OR MIT | app | all | Copyright (c) 2022 Bobby Holley |
| atspi | 0.29.0 | Apache-2.0 OR MIT | app | Linux | Copyright (c) 2022 Tait Hoyem <tait@tait.tech> |
| atspi-common | 0.13.0 | Apache-2.0 OR MIT | app | Linux | Copyright (c) 2022 Tait Hoyem <tait@tait.tech> |
| atspi-proxies | 0.13.0 | Apache-2.0 OR MIT | app | Linux | Copyright (c) 2022 Tait Hoyem <tait@tait.tech> |
| base64 | 0.23.1 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2025 Alice Maz, Marshall Pierce |
| beul | 1.0.0 | MIT OR Apache-2.0 | app | macOS, Windows | Copyright (c) 2023 Bert Peters |
| bit-set | 0.10.0 | Apache-2.0 OR MIT | app | all | Copyright (c) 2026 The Rust Project Developers |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | app | all | Copyright (c) 2023 The Rust Project Developers |
| bitflags | 1.3.2 | MIT/Apache-2.0 | app | macOS | Copyright (c) 2014 The Rust Project Developers |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014 The Rust Project Developers |
| blitz-dom | 0.3.0-beta.2 (git: https://github.com/DioxusLabs/blitz, revision 45496d9288b063719ebeae4e0761273f7881b3d7) | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| blitz-html | 0.3.0-beta.2 (git: https://github.com/DioxusLabs/blitz, revision 45496d9288b063719ebeae4e0761273f7881b3d7) | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| blitz-paint | 0.3.0-beta.2 (git: https://github.com/DioxusLabs/blitz, revision 45496d9288b063719ebeae4e0761273f7881b3d7) | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| blitz-traits | 0.3.0-beta.2 (git: https://github.com/DioxusLabs/blitz, revision 45496d9288b063719ebeae4e0761273f7881b3d7) | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2018-2019 The RustCrypto Project Developers |
| block2 | 0.5.1 | MIT | app | macOS | copyright its authors, see the crate (authors: Steven Sheldon, Mads Marquart) |
| block2 | 0.6.2 | MIT | app | macOS | copyright its authors, see the crate (authors: Mads Marquart) |
| blocking | 1.7.0 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (its Cargo.toml names no authors) |
| brotli-decompressor | 5.0.3 | BSD-3-Clause/MIT | app | all | Copyright (c) 2016 Dropbox, Inc. |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | CLI and app | all | Copyright (c) 2019 Daniel "Lokathor" Gee. |
| byteorder | 1.5.0 | Unlicense OR MIT | app | all | Copyright (c) 2015 Andrew Gallant |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | CLI and app | all | Copyright (c) 2015 Andrew Gallant |
| bytes | 1.12.1 | MIT | CLI and app | all | Copyright (c) 2018 Carl Lerche |
| calloop | 0.13.0 | MIT | app | Linux | Copyright (c) 2018 Victor Berger |
| calloop | 0.14.5 | MIT | app | Linux | Copyright (c) 2018 Victor Berger |
| calloop-wayland-source | 0.3.0 | MIT | app | Linux | Copyright (c) 2023 Kirill Chibisov |
| calloop-wayland-source | 0.4.1 | MIT | app | Linux | Copyright (c) 2023 Kirill Chibisov |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014 Alex Crichton |
| color | 0.3.3 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| color_quant | 1.1.0 | MIT | app | all | Copyright (c) 2016 PistonDevelopers |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina, Taiki Endo, John Nunley) |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | app | macOS | Copyright (c) 2012-2013 Mozilla Foundation |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | app | macOS | Copyright (c) 2012-2013 Mozilla Foundation |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 | app | macOS | Copyright (c) 2012-2013 Mozilla Foundation |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 | app | macOS | Copyright (c) 2012-2013 Mozilla Foundation |
| core_detect | 1.0.0 | MIT/Apache-2.0 | app | Linux, Windows | Copyright (c) 2017-2020 The Rust Project Developers |
| core_maths | 0.1.1 | MIT | app | all | Copyright (c) 2024 Robert Bastian |
| coreaudio-rs | 0.14.2 | MIT/Apache-2.0 | app | macOS | Copyright (c) 2015 (no holder named; authors: mitchmindtree, yupferris) |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2020-2025 The RustCrypto Project Developers |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | app | macOS | Copyright (c) 2019 The Crossbeam Project Developers; Copyright (c) 2009 The Go Authors. All rights reserved. |
| crossbeam-deque | 0.8.8 | MIT OR Apache-2.0 | app | all | Copyright (c) 2019 The Crossbeam Project Developers |
| crossbeam-epoch | 0.9.21 | MIT OR Apache-2.0 | app | all | Copyright (c) 2019 The Crossbeam Project Developers |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | app | all | Copyright (c) 2019 The Crossbeam Project Developers |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2021 RustCrypto Developers |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | app | all | Copyright (c) 2023 Kirill Chibisov |
| dasp_sample | 0.11.0 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: mitchmindtree) |
| data-url | 0.3.2 | MIT OR Apache-2.0 | app | all | Copyright (c) 2013-2025 The rust-url developers |
| debug_timer | 0.1.3 (git: https://github.com/DioxusLabs/blitz, revision 45496d9288b063719ebeae4e0761273f7881b3d7) | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| debug_timer | 0.1.3 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| deranged | 0.5.8 | MIT OR Apache-2.0 | app | all | Copyright (c) 2024 Jacob Pratt et al. |
| derive_more | 2.1.1 | MIT | app | all | Copyright (c) 2016 Jelte Fennema |
| digest | 0.10.7 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2017 Artyom Pavlov |
| dispatch | 0.2.0 | MIT | app | macOS | copyright its authors, see the crate (authors: Steven Sheldon) |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (authors: Mads Marquart, Mary) |
| dlib | 0.5.3 | MIT | app | Linux | Copyright (c) 2015 Victor Berger |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 | app | Linux | Copyright (c) 2020 Ashish Myles and contributors |
| dtoa | 1.0.11 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: David Tolnay) |
| ecolor | 0.36.2 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Emil Ernerfeldt, Andreas Reich) |
| eframe | 0.36.2 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Emil Ernerfeldt) |
| egui | 0.36.2 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Emil Ernerfeldt) |
| egui-wgpu | 0.36.2 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Nils Hasenbanck, embotech, Emil Ernerfeldt) |
| egui-winit | 0.36.2 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Emil Ernerfeldt) |
| either | 1.19.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015 (no holder named, and its Cargo.toml names no authors) |
| emath | 0.36.2 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Emil Ernerfeldt) |
| endi | 1.1.1 | MIT | app | Linux | copyright its authors, see the crate (authors: Zeeshan Ali Khan) |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2017-2023 Maik Klein, Maja Kądziołka |
| epaint | 0.36.2 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Emil Ernerfeldt) |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | CLI and app | all | Copyright (c) 2016--2023 (no holder named, and its Cargo.toml names no authors) |
| errno | 0.3.14 | MIT OR Apache-2.0 | CLI and app | Linux, macOS | Copyright (c) 2014 Chris Wong |
| euclid | 0.22.14 | MIT OR Apache-2.0 | app | all | Copyright (c) 2012-2013 Mozilla Foundation |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina, John Nunley) |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: John Nunley) |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (authors: Stjepan Glavina) |
| fax | 0.2.7 | MIT | app | macOS | Copyright © 2021 The pdf-rs contributers. |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: The image-rs Developers) |
| fearless_simd | 0.4.1 | Apache-2.0 OR MIT | app | all | Copyright (c) 2018 Raph Levien |
| fearless_simd | 0.7.0 | Apache-2.0 OR MIT | app | all | Copyright (c) 2018 Raph Levien |
| fearless_simd | 1.1.0 | Apache-2.0 OR MIT | CLI and app | all | Copyright (c) 2018 Raph Levien |
| filetime | 0.2.29 | MIT/Apache-2.0 | app | all | Copyright (c) 2014 Alex Crichton |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014-2026 Alex Crichton |
| float-cmp | 0.9.0 | MIT | app | all | Copyright (c) 2014-2020 Optimal Computing (NZ) Ltd |
| font-types | 0.12.6 | MIT OR Apache-2.0 | app | all | Copyright (c) 2019 Fontations Developers |
| fontconfig-parser | 0.5.8 | MIT | app | Linux | Copyright (c) 2021 Riey |
| fontdb | 0.24.0 | MIT | app | all | Copyright (c) 2020 Yevhenii Reizner |
| fontique | 0.12.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| foreign-types | 0.5.0 | MIT/Apache-2.0 | app | macOS | Copyright (c) 2017 The foreign-types Developers |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | app | macOS | Copyright (c) 2017 The foreign-types Developers |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | app | all | Copyright (c) 2013-2016 The rust-url developers |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | app | all | Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors |
| generic-array | 0.14.7 | MIT | CLI and app | all | Copyright (c) 2015 Bartłomiej Kamiński |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | app | all | Copyright (c) 2018-2024 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2018-2025 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers |
| gif | 0.14.2 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015 nwin |
| glifo | 0.4.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | app | Linux, Windows | copyright its authors, see the crate (authors: Joshua Groves, Dzmitry Malyshau) |
| gpu-allocator | 0.28.0 | MIT OR Apache-2.0 | app | Linux, Windows | Copyright (c) 2021 Traverse Research B.V. |
| guillotiere | 0.7.0 | MIT/Apache-2.0 | app | all | Copyright (c) 2019 Nicolas Silva |
| half | 2.7.1 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Kathryn Long) |
| harfrust | 0.12.0 | MIT | app | all | Copyright (c) HarfBuzz developers; Copyright (c) 2020 Yevhenii Reizner |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016 Amanieu d'Antras |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2016 Amanieu d'Antras |
| hayro-ccitt | 0.4.0 | Apache-2.0 OR MIT | CLI and app | all | Copyright (c) The Hayro Authors |
| hayro-jbig2 | 0.3.1 | Apache-2.0 OR MIT | CLI and app | all | Copyright (c) The Hayro Authors |
| hayro-jpeg2000 | 0.4.1 | Apache-2.0 OR MIT | CLI and app | all | Copyright (c) The Hayro Authors |
| hayro-syntax | 0.8.0 | Apache-2.0 OR MIT | CLI and app | all | Copyright (c) The Hayro Authors |
| hex | 0.4.3 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2013-2014 The Rust Project Developers.; Copyright (c) 2015-2020 The rust-hex Developers |
| html-escape | 0.2.15 | MIT | app | all | Copyright (c) 2020 magiclen.org (Ron Li) |
| html5ever | 0.40.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2014 The html5ever Project Developers |
| http | 1.5.0 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2017 http-rs authors |
| httparse | 1.10.1 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2015-2025 Sean McArthur |
| idna | 1.1.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2013-2025 The rust-url developers |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | app | all | Copyright (c) The rust-url developers |
| image | 0.25.10 | MIT OR Apache-2.0 | CLI and app | all | copyright its authors, see the crate (authors: The image-rs Developers) |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| imagesize | 0.15.0 | MIT | app | all | Copyright (c) 2017 Maiddog |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | CLI and app | all | Copyright (c) 2016--2017 (no holder named, and its Cargo.toml names no authors) |
| itertools | 0.14.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015 (no holder named; authors: bluss) |
| itertools | 0.15.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015 (no holder named; authors: bluss) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | CLI and app | all | copyright its authors, see the crate (authors: David Tolnay) |
| keyboard-types | 0.7.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2017 Pyfisch |
| keyboard-types | 0.8.3 | MIT OR Apache-2.0 | app | macOS | Copyright (c) 2017 Pyfisch |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 | app | Linux, Windows | copyright its authors, see the crate (authors: Timothée Haudebourg, Sean Kerr) |
| kurbo | 0.13.1 | Apache-2.0 OR MIT | app | all | Copyright (c) 2018 Raph Levien |
| libc | 0.2.190 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) The Rust Project Developers |
| libm | 0.2.16 | MIT | app | all | Copyright (c) 2018 Jorge Aparicio; Copyright © 2005-2020 Rich Felker, et al.; Copyright © 1993,2004 Sun Microsystems or; Copyright © 2003-2011 David Schultz or; Copyright © 2003-2009 Steven G. Kargl or; Copyright © 2003-2009 Bruce D. Evans or; Copyright © 2008 Stephen L. Moshier or; Copyright © 2017-2018 Arm Limited |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | CLI and app | Linux | copyright its authors, see the crate (authors: Dan Gohman) |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Dan Gohman) |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016 The Rust Project Developers |
| log | 0.4.34 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014 The Rust Project Developers |
| mach2 | 0.6.0 | BSD-2-Clause OR MIT OR Apache-2.0 | app | macOS | Copyright (c) 2019 Nick Fitzgerald, 2021 Yuki Okushi |
| markup5ever | 0.40.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2014 The html5ever Project Developers |
| memchr | 2.8.3 | Unlicense OR MIT | CLI and app | all | Copyright (c) 2015 Andrew Gallant |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | app | all | Copyright (c) 2020 Yevhenii Reizner; Copyright (c) 2015 Dan Burkert |
| mime | 0.3.17 | MIT OR Apache-2.0 | app | all | Copyright (c) 2014 Sean McArthur |
| minisign-verify | 0.2.5 | MIT | app | all | Copyright (c) 2019-2025 Frank Denis; Copyright (c) 2006-2009 Graydon Hoare; Copyright (c) 2009-2013 Mozilla Foundation |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | CLI and app | all | Copyright (c) 2017 Frommi; Copyright (c) 2017-2024 oyvindln; Copyright (c) 2020 Frommi |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | app | all | Copyright (c) 2017 Frommi; Copyright (c) 2017-2024 oyvindln; Copyright (c) 2020 Frommi |
| muda | 0.21.2 | Apache-2.0 OR MIT | app | macOS | Copyright (c) 2022-2022 Tauri Programme within The Commons Conservancy |
| naga | 30.0.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2025 The gfx-rs developers |
| naga-types | 30.0.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2025 The gfx-rs developers |
| new_debug_unreachable | 1.0.6 | MIT | app | all | Copyright (c) 2015 Jonathan Reem |
| nix | 0.31.3 | MIT | CLI and app | macOS | Copyright (c) 2015 Carl Lerche + nix-rust Authors |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (authors: Parity Technologies) |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | app | all | Copyright (c) Jacob Pratt |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | app | all | Copyright (c) 2014 The Rust Project Developers |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014 The Rust Project Developers |
| num_cpus | 1.17.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015-2025 Sean McArthur |
| objc-sys | 0.3.5 | MIT | app | macOS | copyright its authors, see the crate (authors: Mads Marquart) |
| objc2 | 0.5.3 | MIT | app | macOS | copyright its authors, see the crate (authors: Steven Sheldon, Mads Marquart) |
| objc2 | 0.6.5 | MIT | app | macOS | copyright its authors, see the crate (authors: Mads Marquart) |
| objc2-app-kit | 0.2.2 | MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-audio-toolbox | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-cloud-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-core-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-core-audio-types | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-core-data | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-core-image | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-core-text | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-core-video | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-encode | 4.1.0 | MIT | app | macOS | copyright its authors, see the crate (authors: Mads Marquart) |
| objc2-foundation | 0.2.2 | MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-foundation | 0.3.2 | MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Aleksey Kladov) |
| ordered-float | 5.5.0 | MIT | app | Linux, Windows | Copyright (c) 2015 Jonathan Reem |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: Daniel De Graaf, Zeeshan Ali Khan) |
| parking | 2.2.1 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (authors: Stjepan Glavina, The Rust Project Developers) |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016 The Rust Project Developers |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016 The Rust Project Developers |
| parlance | 0.1.1 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| parley | 0.12.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| parley_data | 0.12.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| parley_emoji | 0.12.0 | MIT | app | all | Copyright (c) 2026 Christian Hansen <chansen@cpan.org> |
| parley_engine | 0.12.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| peniko | 0.6.1 | Apache-2.0 OR MIT | app | all | Copyright (c) 2018 Raph Levien |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2013-2025 The rust-url developers |
| phf | 0.13.1 | MIT | app | Linux | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi |
| phf | 0.14.0 | MIT | app | all | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi |
| phf_shared | 0.13.1 | MIT | app | Linux | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi |
| phf_shared | 0.14.0 | MIT | app | all | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi |
| pico-args | 0.5.0 | MIT | app | all | Copyright (c) 2019 Yevhenii Reizner |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| piper | 0.2.5 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina, John Nunley) |
| plist | 1.10.1 | MIT | app | all | Copyright (c) 2015 Edward Barnard |
| png | 0.18.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015 nwin |
| polling | 3.11.0 | Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Stjepan Glavina, John Nunley) |
| pollster | 0.4.0 | Apache-2.0/MIT | app | Linux | Copyright (c) 2020-2021 Joshua Barretto |
| pollster | 1.0.1 | Apache-2.0/MIT | app | all | Copyright (c) 2020-2021 Joshua Barretto |
| polycool | 0.4.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2018 Raph Levien |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2023 Jacob Pratt et al. |
| precomputed-hash | 0.1.1 | MIT | app | all | Copyright (c) 2017 Emilio Cobos Álvarez |
| presser | 0.3.1 | MIT OR Apache-2.0 | app | Linux, Windows | Copyright (c) 2019 Embark Studios |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: David Tolnay, Alex Crichton) |
| profiling | 1.0.18 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Philip Degarmo) |
| quick-error | 2.0.1 | MIT/Apache-2.0 | app | all | Copyright (c) 2015 The quick-error Developers |
| quick-xml | 0.42.0 | MIT | app | all | Copyright (c) 2016 Johann Tuffe |
| quote | 1.0.47 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: David Tolnay) |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 | app | Windows | Copyright (c) 2023 The gfx-rs developers |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | app | all | Copyright (c) 2019 Osspial; Copyright (c) 2020 Osspial |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 | app | macOS | copyright its authors, see the crate (its Cargo.toml names no authors) |
| rayon | 1.12.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2010 The Rust Project Developers |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2010 The Rust Project Developers |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2019 Fontations Developers |
| regex | 1.13.1 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014 The Rust Project Developers |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014 The Rust Project Developers |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014 The Rust Project Developers |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | app | Linux, Windows | Copyright (c) 2022 Eyal Kalderon |
| rfd | 0.17.2 | MIT | app | all | Copyright (c) 2022 Bartłomiej Maryńczak |
| roxmltree | 0.20.0 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2018 Yevhenii Reizner |
| roxmltree | 0.21.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2018 Yevhenii Reizner |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | app | all | copyright its authors, see the crate (authors: The Rust Project Developers) |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | CLI and app | all | copyright its authors, see the crate (authors: The Rust Project Developers) |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | app | Linux | copyright its authors, see the crate (authors: Dan Gohman, Jakub Konka) |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | CLI and app | Linux, macOS | copyright its authors, see the crate (authors: Dan Gohman, Jakub Konka) |
| rustls | 0.23.45 | Apache-2.0 OR ISC OR MIT | app | all | Copyright (c) 2016, Joseph Birr-Pixton <jpixton@gmail.com>; Copyright (c) 2016 Joseph Birr-Pixton <jpixton@gmail.com> |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2023 Dirkjan Ochtman <dirkjan@ochtman.nl> |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 | app | Linux | Copyright (c) 2014 Alex Crichton |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers |
| sctk-adwaita | 0.10.1 | MIT | app | Linux | Copyright (c) 2022 Bartłomiej Maryńczak |
| semver | 1.0.28 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: David Tolnay) |
| serde | 1.0.229 | MIT OR Apache-2.0 | CLI and app | all | copyright its authors, see the crate (authors: Erick Tryzelaar, David Tolnay) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | CLI and app | all | copyright its authors, see the crate (authors: Erick Tryzelaar, David Tolnay) |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | CLI and app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| servo_arc | 0.5.0 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2006-2009 Graydon Hoare; Copyright (c) 2009-2013 Mozilla Foundation; Copyright (c) 2016 Artyom Pavlov |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2017 tokio-jsonrpc developers |
| simd-adler32 | 0.3.10 | MIT | app | all | Copyright (c) [2021] [Marvin Countryman] |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Hans Kratz) |
| simplecss | 0.2.2 | Apache-2.0 OR MIT | app | all | Copyright (c) 2018 Reizner Evgeniy |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Frank Denis) |
| skrifa | 0.44.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2019 Fontations Developers |
| slab | 0.4.12 | MIT | app | Linux | Copyright (c) 2019 Carl Lerche |
| smallbitvec | 2.6.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2017 Matt Brubeck |
| smallvec | 1.16.3 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2018 The Servo Project Developers |
| smithay-client-toolkit | 0.19.2 | MIT | app | Linux | Copyright (c) 2018 Victor Berger |
| smithay-client-toolkit | 0.20.0 | MIT | app | Linux | Copyright (c) 2018 Victor Berger |
| smithay-clipboard | 0.7.3 | MIT | app | Linux | Copyright (c) 2018 Lucas Timmins & Victor Berger |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Aleksey Kladov) |
| smol_str | 0.3.6 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Aleksey Kladov, Lukas Wirth) |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2017 Robert Grosse |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2017 Nikolai Vazquez |
| strict-num | 0.1.1 | MIT | app | all | Copyright (c) 2022 Yevhenii Reizner |
| string_cache | 0.11.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2012-2013 Mozilla Foundation |
| strum | 0.28.0 | MIT | app | all | Copyright (c) 2019 Peter Glotfelty |
| stylo_malloc_size_of | 0.22.0 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| stylo_taffy | 0.3.0-beta.2 (git: https://github.com/DioxusLabs/blitz, revision 45496d9288b063719ebeae4e0761273f7881b3d7) | MIT OR Apache-2.0 OR MPL-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| svgtypes | 0.16.1 | Apache-2.0 OR MIT | app | all | Copyright (c) 2018 Yevhenii Reizner |
| syn | 3.0.8 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: David Tolnay) |
| taffy | 0.14.0 (git: https://github.com/DioxusLabs/taffy, revision 7d33901cc383489d9945ae796a1ceed0a935ef66) | MIT | app | all | Copyright (c) 2018 Visly Inc.; Copyright (c) 2026 Taffy Authors |
| tar | 0.4.46 | MIT OR Apache-2.0 | app | all | Copyright (c) The tar-rs Project Contributors |
| tendril | 0.5.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015 Keegan McAllister |
| thin-vec | 0.2.21 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: Aria Beingessner) |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: David Tolnay) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | app | all | copyright its authors, see the crate (authors: David Tolnay) |
| thread_local | 1.1.10 | MIT OR Apache-2.0 | app | all | Copyright (c) 2016 The Rust Project Developers |
| tiff | 0.11.3 | MIT | app | macOS | Copyright (c) 2018 PistonDevelopers |
| time | 0.3.55 | MIT OR Apache-2.0 | app | all | Copyright (c) Jacob Pratt et al. |
| time-core | 0.1.9 | MIT OR Apache-2.0 | app | all | Copyright (c) Jacob Pratt et al. |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | CLI and app | all | Copyright (c) 2019 Daniel "Lokathor" Gee. |
| tracing | 0.1.44 | MIT | app | all | Copyright (c) 2019 Tokio Contributors |
| tracing-core | 0.1.36 | MIT | app | all | Copyright (c) 2019 Tokio Contributors |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 | app | Linux | Copyright (c) 2018 Yevhenii Reizner |
| type-map | 0.5.1 | MIT/Apache-2.0 | app | all | Copyright (c) 2022 Jacob Brown; Copyright (c) 2017-NOW Actix Team |
| typed-path | 0.12.3 | MIT OR Apache-2.0 | CLI and app | all | copyright its authors, see the crate (authors: Chip Senkbeil) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2014 Paho Lurie-Gregg |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 | app | all | This project is copyright 2015, The Servo Project Developers (given in the; Copyright (c) 2015 The Rust Project Developers |
| unicode-normalization | 0.1.25 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2015 The Rust Project Developers |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 | app | all | Copyright (c) 2019 Manish Goregaokar |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015 The Rust Project Developers |
| unicode-vo | 0.1.0 | MIT/Apache-2.0 | app | all | Copyright (c) 2018 Reizner Evgeniy |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015 The Rust Project Developers |
| ureq | 3.4.2 | MIT OR Apache-2.0 | CLI and app | all | Copyright (c) 2019 Martin Algesten |
| ureq-proto | 0.6.4 | MIT OR Apache-2.0 | CLI and app | all | copyright its authors, see the crate (authors: Martin Algesten) |
| url | 2.5.8 | MIT OR Apache-2.0 | app | all | Copyright (c) 2013-2025 The rust-url developers |
| usvg | 0.48.1 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| utf8-zero | 0.8.1 | MIT OR Apache-2.0 | CLI and app | all | copyright its authors, see the crate (authors: Simon Sapin, Martin Algesten) |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (authors: Henri Sivonen) |
| uuid | 1.28.0 | Apache-2.0 OR MIT | app | all | Copyright (c) 2014 The Rust Project Developers; Copyright (c) 2018 Ashley Mannix, Christopher Armstrong, Dylan DPC, Hunar Roop Kahlon |
| vello_common | 0.1.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| vello_common | 0.3.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| vello_cpu | 0.1.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| vello_cpu | 0.3.0 | Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| void | 1.0.2 | MIT | app | all | copyright its authors, see the crate (authors: Jonathan Reem) |
| wayland-backend | 0.3.17 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| wayland-client | 0.31.15 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| wayland-csd-frame | 0.3.0 | MIT | app | Linux | Copyright (c) 2023 Kirill Chibisov |
| wayland-cursor | 0.31.14 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| wayland-protocols | 0.32.13 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| wayland-protocols-experimental | 20250721.0.1 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| wayland-protocols-misc | 0.3.12 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| wayland-protocols-plasma | 0.3.12 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| wayland-protocols-wlr | 0.3.12 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| wayland-sys | 0.31.11 | MIT | app | Linux | Copyright (c) 2015 Elinor Berger |
| web-time | 1.1.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2023 dAxpeDDa |
| web_atoms | 0.3.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2014 The html5ever Project Developers |
| webbrowser | 1.2.4 | MIT OR Apache-2.0 | app | all | Copyright (c) 2015-2022 Amod Malviya |
| weezl | 0.1.12 | MIT OR Apache-2.0 | app | all | Copyright (c) HeroicKatora 2020 |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2025 The gfx-rs developers |
| wgpu-core | 30.0.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2025 The gfx-rs developers |
| wgpu-core-deps-apple | 30.0.1 | MIT OR Apache-2.0 | app | macOS | Copyright (c) 2025 The gfx-rs developers |
| wgpu-core-deps-windows-linux-android | 30.0.1 | MIT OR Apache-2.0 | app | Linux, Windows | Copyright (c) 2025 The gfx-rs developers |
| wgpu-hal | 30.0.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2025 The gfx-rs developers |
| wgpu-naga-bridge | 30.0.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2025 The gfx-rs developers |
| wgpu-types | 30.0.1 | MIT OR Apache-2.0 | app | all | Copyright (c) 2025 The gfx-rs developers |
| windows | 0.62.2 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 | app | Windows | Copyright (c) Microsoft Corporation. |
| winnow | 1.0.4 | MIT | app | Linux | copyright its authors, see the crate (its Cargo.toml names no authors) |
| wuff | 0.2.9 | MIT | app | all | copyright its authors, see the crate (authors: Nico Burns) |
| x11-dl | 2.21.0 | MIT | app | Linux | copyright its authors, see the crate (authors: daggerbot, Erle Pereira, AltF02) |
| x11rb | 0.13.2 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: Uli Schlachter, Eduardo Sánchez Muñoz, notgull) |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: Uli Schlachter, Eduardo Sánchez Muñoz, notgull) |
| xcursor | 0.3.11 | MIT | app | Linux | Copyright (c) 2020 Samuele Esposito |
| xkbcommon-dl | 0.4.2 | MIT | app | Linux | Copyright (c) 2023 Kirill Chibisov |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | app | Linux | Copyright (c) 2022-2023 John Nunley |
| xml5ever | 0.40.0 | MIT OR Apache-2.0 | app | all | Copyright (c) 2014 The html5ever Project Developers |
| xmlwriter | 0.1.0 | MIT | app | all | Copyright (c) 2019 Reizner Evgeniy |
| yeslogic-fontconfig-sys | 6.0.1 | MIT | app | Linux | copyright its authors, see the crate (authors: Austin Bonander, The Servo Project Developers, YesLogic Pty. Ltd.) |
| zbus | 5.19.0 | MIT | app | Linux | Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors |
| zbus-lockstep | 0.5.2 | MIT | app | Linux | copyright its authors, see the crate (authors: Luuk van der Duim, Tait Hoyem) |
| zbus_names | 4.3.4 | MIT | app | Linux | Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors |
| zbus_xml | 5.2.1 | MIT | app | Linux | Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors |
| zcheapstr | 1.1.0 | MIT | app | Linux | Copyright (c) 2026 Zeeshan Ali Khan & zcheapstr contributors |
| zerocopy | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | app | all | Copyright (c) 2018-2026 The RustCrypto Project Developers |
| zip | 9.0.2 | MIT | CLI and app | all | Copyright (c) 2014 Mathijs van de Nes |
| zmij | 1.0.23 | MIT | CLI and app | all | copyright its authors, see the crate (authors: David Tolnay) |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | CLI and app | all | Copyright (c) zune-image developers |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | CLI and app | all | Copyright (c) zune-image developers |
| zvariant | 5.15.0 | MIT | app | Linux | Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors |
| zvariant_utils | 4.2.0 | MIT | app | Linux | copyright its authors, see the crate (authors: Zeeshan Ali Khan, turbocooler) |

### Apache-2.0 (15)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| ab_glyph | 0.2.32 | Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: Alex Butler) |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: Alex Butler) |
| accesskit_winit | 0.32.2 | Apache-2.0 | app | all | copyright its authors, see the crate (authors: The AccessKit contributors) |
| codespan-reporting | 0.13.1 | Apache-2.0 | app | all | copyright its authors, see the crate (authors: Brendan Zabarauskas) |
| cpal | 0.18.2 | Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| gethostname | 1.1.0 | Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: Sebastian Wiesner) |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | app | Windows | copyright its authors, see the crate (authors: Kirill Chibisov) |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | CLI and app | all | Copyright (c) Radzivon Bartoshyk. All rights reserved. |
| mundy | 0.2.3 | Apache-2.0 | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| owned_ttf_parser | 0.25.1 | Apache-2.0 | app | Linux | copyright its authors, see the crate (authors: Alex Butler) |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | CLI and app | all | Copyright (c) Radzivon Bartoshyk. All rights reserved. |
| self_cell | 1.3.0 | Apache-2.0 OR GPL-2.0-only | app | all | copyright its authors, see the crate (authors: Lukas Bergdoll) |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | app | Linux, Windows | copyright its authors, see the crate (authors: Lei Zhang) |
| unicode-general-category | 1.1.0 | Apache-2.0 | CLI and app | all | copyright its authors, see the crate (authors: YesLogic Pty. Ltd.) |
| winit | 0.30.13 | Apache-2.0 | app | all | copyright its authors, see the crate (authors: The winit contributors, Pierre Krieger) |

### Unicode-3.0 (21)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| icu_casemap | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_casemap_data | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_collections | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_locale_core | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_locale_fallback | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_locale_fallback_data | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_normalizer | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_properties | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_properties_data | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_provider | 2.3.1 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_segmenter | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| icu_segmenter_data | 2.3.0 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| litemap | 0.8.3 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| potential_utf | 0.1.6 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| tinystr | 0.8.4 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| writeable | 0.6.4 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| yoke | 0.8.3 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| zerofrom | 0.1.8 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| zerotrie | 0.2.5 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |
| zerovec | 0.11.8 | Unicode-3.0 | app | all | Copyright © 2020-2024 Unicode, Inc. |

### MPL-2.0 (11)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| app_units | 0.7.8 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| cssparser | 0.38.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: Simon Sapin) |
| dtoa-short | 0.3.5 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: Xidorn Quan) |
| selectors | 0.41.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| stylo | 0.22.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| stylo_atoms | 0.22.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| stylo_dom | 0.22.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| stylo_static_prefs | 0.22.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| stylo_traits | 0.22.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| to_shmem | 0.6.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers) |
| uluru | 3.1.0 | MPL-2.0 | app | all | copyright its authors, see the crate (authors: The Servo Project Developers, Matt Brubeck) |

### BSD-3-Clause (5)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| alloc-no-stdlib | 2.0.4 | BSD-3-Clause | app | all | Copyright (c) 2016 Dropbox, Inc. |
| subtle | 2.6.1 | BSD-3-Clause | app | all | Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved.; Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved. |
| tiny-skia | 0.11.4 | BSD-3-Clause | app | Linux | Copyright (c) 2011 Google Inc. All rights reserved.; Copyright (c) 2020 Yevhenii Reizner All rights reserved. |
| tiny-skia-path | 0.11.4 | BSD-3-Clause | app | Linux | Copyright (c) 2011 Google Inc. All rights reserved.; Copyright (c) 2020 Yevhenii Reizner All rights reserved. |
| tiny-skia-path | 0.12.0 | BSD-3-Clause | app | all | Copyright (c) 2011 Google Inc. All rights reserved.; Copyright (c) 2020 Yevhenii Reizner All rights reserved. |

### BSD-2-Clause (1)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| arrayref | 0.3.9 | BSD-2-Clause | app | all | Copyright (c) 2015 David Roundy <roundyd@physics.oregonstate.edu> |

### ISC (3)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| libloading | 0.8.9 | ISC | app | all | Copyright © 2015, Simonas Kazlauskas |
| rustls-webpki | 0.103.15 | ISC | app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |
| untrusted | 0.9.0 | ISC | app | all | copyright its authors, see the crate (authors: Brian Smith) |

### Zlib (3)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| foldhash | 0.2.0 | Zlib | CLI and app | all | Copyright (c) 2024 Orson Peters |
| slotmap | 1.1.1 | Zlib | app | all | Copyright (c) 2021 Orson Peters <orsonpeters@gmail.com> |
| zlib-rs | 0.6.8 | Zlib | CLI and app | all | copyright its authors, see the crate (its Cargo.toml names no authors) |

### BSL-1.0 (2)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| clipboard-win | 5.4.1 | BSL-1.0 | app | Windows | copyright its authors, see the crate (authors: Douman) |
| error-code | 3.4.0 | BSL-1.0 | app | Windows | copyright its authors, see the crate (authors: Douman) |

### CDLA-Permissive-2.0 (1)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | app | all | Mozilla's root certificates, derived from the Common CA Database (CCADB) data under the CCADB Data Usage Terms (authors not named) |

### Apache-2.0 + ISC (1)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| ring | 0.17.14 | Apache-2.0 AND ISC | app | all | Copyright 2015-2025 Brian Smith (ISC parts); code from BoringSSL and OpenSSL, copyright The BoringSSL Authors and The OpenSSL Project Authors (Apache-2.0 parts); fiat-crypto code, Copyright 2015-2020 the fiat-crypto authors (Apache-2.0); see "ring" below |

### Apache-2.0 + MIT (1)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| dpi | 0.1.2 | Apache-2.0 AND MIT | app | all | Copyright (c) 2018 Jorge Aparicio; Copyright © 2005-2020 Rich Felker, et al.; Copyright © 1993,2004 Sun Microsystems or; Copyright © 2003-2011 David Schultz or; Copyright © 2003-2009 Steven G. Kargl or; Copyright © 2003-2009 Bruce D. Evans or; Copyright © 2008 Stephen L. Moshier or; Copyright © 2017-2018 Arm Limited |

### MIT + BSD-3-Clause (1)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| encoding_rs | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause | app | all | encoding_rs is copyright Mozilla Foundation.; Copyright © WHATWG (Apple, Google, Mozilla, Microsoft). |

### MIT + OFL-1.1 + Ubuntu-font-1.0 (1)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| epaint_default_fonts | 0.36.2 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | app | all | Hack: Copyright (c) 2018 Source Foundry Authors, and Copyright (c) 2003 by Bitstream, Inc.; Noto Emoji: Copyright 2013 Google Inc.; Ubuntu Light: Copyright 2011 Canonical Ltd.; emoji-icon-font: Copyright (c) 2014 John Slegers; see "The fonts inside egui" below |

### MIT + Unicode-3.0 (1)

| Crate | Version | Licence as declared | Ships in | Platforms | Copyright |
| --- | --- | --- | --- | --- | --- |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | app | all | Copyright © 1991-2023 Unicode, Inc. |

---

## Licence texts

Each quoted verbatim from the file named. A crate's own copyright lines are in its row of the tables above.

### MIT License

The text every MIT crate above is licensed under. Each crate's own copyright line, which this text calls "the above copyright notice", is in its row of the tables. Quoted from the blitz repository's LICENSE-MIT.

```text
Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

### Apache License 2.0

Quoted from serde 1.0.229's LICENSE-APACHE. None of the crates above that Method Reader uses under Apache-2.0 ships a NOTICE file.

```text
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS
```

### Unicode License v3

Quoted from icu_collections 2.3.0's LICENSE; the ICU4X crates all carry this file. unicode-ident carries the same licence for its Unicode data, with the line "Copyright © 1991-2023 Unicode, Inc.".

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### Mozilla Public License 2.0

Quoted from cssparser 0.38.0's LICENSE. The MPL-2.0 crates are used unmodified, and their source is available from crates.io at the versions named above.

```text
Mozilla Public License Version 2.0
==================================

1. Definitions
--------------

1.1. "Contributor"
    means each individual or legal entity that creates, contributes to
    the creation of, or owns Covered Software.

1.2. "Contributor Version"
    means the combination of the Contributions of others (if any) used
    by a Contributor and that particular Contributor's Contribution.

1.3. "Contribution"
    means Covered Software of a particular Contributor.

1.4. "Covered Software"
    means Source Code Form to which the initial Contributor has attached
    the notice in Exhibit A, the Executable Form of such Source Code
    Form, and Modifications of such Source Code Form, in each case
    including portions thereof.

1.5. "Incompatible With Secondary Licenses"
    means

    (a) that the initial Contributor has attached the notice described
        in Exhibit B to the Covered Software; or

    (b) that the Covered Software was made available under the terms of
        version 1.1 or earlier of the License, but not also under the
        terms of a Secondary License.

1.6. "Executable Form"
    means any form of the work other than Source Code Form.

1.7. "Larger Work"
    means a work that combines Covered Software with other material, in 
    a separate file or files, that is not Covered Software.

1.8. "License"
    means this document.

1.9. "Licensable"
    means having the right to grant, to the maximum extent possible,
    whether at the time of the initial grant or subsequently, any and
    all of the rights conveyed by this License.

1.10. "Modifications"
    means any of the following:

    (a) any file in Source Code Form that results from an addition to,
        deletion from, or modification of the contents of Covered
        Software; or

    (b) any new file in Source Code Form that contains any Covered
        Software.

1.11. "Patent Claims" of a Contributor
    means any patent claim(s), including without limitation, method,
    process, and apparatus claims, in any patent Licensable by such
    Contributor that would be infringed, but for the grant of the
    License, by the making, using, selling, offering for sale, having
    made, import, or transfer of either its Contributions or its
    Contributor Version.

1.12. "Secondary License"
    means either the GNU General Public License, Version 2.0, the GNU
    Lesser General Public License, Version 2.1, the GNU Affero General
    Public License, Version 3.0, or any later versions of those
    licenses.

1.13. "Source Code Form"
    means the form of the work preferred for making modifications.

1.14. "You" (or "Your")
    means an individual or a legal entity exercising rights under this
    License. For legal entities, "You" includes any entity that
    controls, is controlled by, or is under common control with You. For
    purposes of this definition, "control" means (a) the power, direct
    or indirect, to cause the direction or management of such entity,
    whether by contract or otherwise, or (b) ownership of more than
    fifty percent (50%) of the outstanding shares or beneficial
    ownership of such entity.

2. License Grants and Conditions
--------------------------------

2.1. Grants

Each Contributor hereby grants You a world-wide, royalty-free,
non-exclusive license:

(a) under intellectual property rights (other than patent or trademark)
    Licensable by such Contributor to use, reproduce, make available,
    modify, display, perform, distribute, and otherwise exploit its
    Contributions, either on an unmodified basis, with Modifications, or
    as part of a Larger Work; and

(b) under Patent Claims of such Contributor to make, use, sell, offer
    for sale, have made, import, and otherwise transfer either its
    Contributions or its Contributor Version.

2.2. Effective Date

The licenses granted in Section 2.1 with respect to any Contribution
become effective for each Contribution on the date the Contributor first
distributes such Contribution.

2.3. Limitations on Grant Scope

The licenses granted in this Section 2 are the only rights granted under
this License. No additional rights or licenses will be implied from the
distribution or licensing of Covered Software under this License.
Notwithstanding Section 2.1(b) above, no patent license is granted by a
Contributor:

(a) for any code that a Contributor has removed from Covered Software;
    or

(b) for infringements caused by: (i) Your and any other third party's
    modifications of Covered Software, or (ii) the combination of its
    Contributions with other software (except as part of its Contributor
    Version); or

(c) under Patent Claims infringed by Covered Software in the absence of
    its Contributions.

This License does not grant any rights in the trademarks, service marks,
or logos of any Contributor (except as may be necessary to comply with
the notice requirements in Section 3.4).

2.4. Subsequent Licenses

No Contributor makes additional grants as a result of Your choice to
distribute the Covered Software under a subsequent version of this
License (see Section 10.2) or under the terms of a Secondary License (if
permitted under the terms of Section 3.3).

2.5. Representation

Each Contributor represents that the Contributor believes its
Contributions are its original creation(s) or it has sufficient rights
to grant the rights to its Contributions conveyed by this License.

2.6. Fair Use

This License is not intended to limit any rights You have under
applicable copyright doctrines of fair use, fair dealing, or other
equivalents.

2.7. Conditions

Sections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted
in Section 2.1.

3. Responsibilities
-------------------

3.1. Distribution of Source Form

All distribution of Covered Software in Source Code Form, including any
Modifications that You create or to which You contribute, must be under
the terms of this License. You must inform recipients that the Source
Code Form of the Covered Software is governed by the terms of this
License, and how they can obtain a copy of this License. You may not
attempt to alter or restrict the recipients' rights in the Source Code
Form.

3.2. Distribution of Executable Form

If You distribute Covered Software in Executable Form then:

(a) such Covered Software must also be made available in Source Code
    Form, as described in Section 3.1, and You must inform recipients of
    the Executable Form how they can obtain a copy of such Source Code
    Form by reasonable means in a timely manner, at a charge no more
    than the cost of distribution to the recipient; and

(b) You may distribute such Executable Form under the terms of this
    License, or sublicense it under different terms, provided that the
    license for the Executable Form does not attempt to limit or alter
    the recipients' rights in the Source Code Form under this License.

3.3. Distribution of a Larger Work

You may create and distribute a Larger Work under terms of Your choice,
provided that You also comply with the requirements of this License for
the Covered Software. If the Larger Work is a combination of Covered
Software with a work governed by one or more Secondary Licenses, and the
Covered Software is not Incompatible With Secondary Licenses, this
License permits You to additionally distribute such Covered Software
under the terms of such Secondary License(s), so that the recipient of
the Larger Work may, at their option, further distribute the Covered
Software under the terms of either this License or such Secondary
License(s).

3.4. Notices

You may not remove or alter the substance of any license notices
(including copyright notices, patent notices, disclaimers of warranty,
or limitations of liability) contained within the Source Code Form of
the Covered Software, except that You may alter any license notices to
the extent required to remedy known factual inaccuracies.

3.5. Application of Additional Terms

You may choose to offer, and to charge a fee for, warranty, support,
indemnity or liability obligations to one or more recipients of Covered
Software. However, You may do so only on Your own behalf, and not on
behalf of any Contributor. You must make it absolutely clear that any
such warranty, support, indemnity, or liability obligation is offered by
You alone, and You hereby agree to indemnify every Contributor for any
liability incurred by such Contributor as a result of warranty, support,
indemnity or liability terms You offer. You may include additional
disclaimers of warranty and limitations of liability specific to any
jurisdiction.

4. Inability to Comply Due to Statute or Regulation
---------------------------------------------------

If it is impossible for You to comply with any of the terms of this
License with respect to some or all of the Covered Software due to
statute, judicial order, or regulation then You must: (a) comply with
the terms of this License to the maximum extent possible; and (b)
describe the limitations and the code they affect. Such description must
be placed in a text file included with all distributions of the Covered
Software under this License. Except to the extent prohibited by statute
or regulation, such description must be sufficiently detailed for a
recipient of ordinary skill to be able to understand it.

5. Termination
--------------

5.1. The rights granted under this License will terminate automatically
if You fail to comply with any of its terms. However, if You become
compliant, then the rights granted under this License from a particular
Contributor are reinstated (a) provisionally, unless and until such
Contributor explicitly and finally terminates Your grants, and (b) on an
ongoing basis, if such Contributor fails to notify You of the
non-compliance by some reasonable means prior to 60 days after You have
come back into compliance. Moreover, Your grants from a particular
Contributor are reinstated on an ongoing basis if such Contributor
notifies You of the non-compliance by some reasonable means, this is the
first time You have received notice of non-compliance with this License
from such Contributor, and You become compliant prior to 30 days after
Your receipt of the notice.

5.2. If You initiate litigation against any entity by asserting a patent
infringement claim (excluding declaratory judgment actions,
counter-claims, and cross-claims) alleging that a Contributor Version
directly or indirectly infringes any patent, then the rights granted to
You by any and all Contributors for the Covered Software under Section
2.1 of this License shall terminate.

5.3. In the event of termination under Sections 5.1 or 5.2 above, all
end user license agreements (excluding distributors and resellers) which
have been validly granted by You or Your distributors under this License
prior to termination shall survive termination.

************************************************************************
*                                                                      *
*  6. Disclaimer of Warranty                                           *
*  -------------------------                                           *
*                                                                      *
*  Covered Software is provided under this License on an "as is"       *
*  basis, without warranty of any kind, either expressed, implied, or  *
*  statutory, including, without limitation, warranties that the       *
*  Covered Software is free of defects, merchantable, fit for a        *
*  particular purpose or non-infringing. The entire risk as to the     *
*  quality and performance of the Covered Software is with You.        *
*  Should any Covered Software prove defective in any respect, You     *
*  (not any Contributor) assume the cost of any necessary servicing,   *
*  repair, or correction. This disclaimer of warranty constitutes an   *
*  essential part of this License. No use of any Covered Software is   *
*  authorized under this License except under this disclaimer.         *
*                                                                      *
************************************************************************

************************************************************************
*                                                                      *
*  7. Limitation of Liability                                          *
*  --------------------------                                          *
*                                                                      *
*  Under no circumstances and under no legal theory, whether tort      *
*  (including negligence), contract, or otherwise, shall any           *
*  Contributor, or anyone who distributes Covered Software as          *
*  permitted above, be liable to You for any direct, indirect,         *
*  special, incidental, or consequential damages of any character      *
*  including, without limitation, damages for lost profits, loss of    *
*  goodwill, work stoppage, computer failure or malfunction, or any    *
*  and all other commercial damages or losses, even if such party      *
*  shall have been informed of the possibility of such damages. This   *
*  limitation of liability shall not apply to liability for death or   *
*  personal injury resulting from such party's negligence to the       *
*  extent applicable law prohibits such limitation. Some               *
*  jurisdictions do not allow the exclusion or limitation of           *
*  incidental or consequential damages, so this exclusion and          *
*  limitation may not apply to You.                                    *
*                                                                      *
************************************************************************

8. Litigation
-------------

Any litigation relating to this License may be brought only in the
courts of a jurisdiction where the defendant maintains its principal
place of business and such litigation shall be governed by laws of that
jurisdiction, without reference to its conflict-of-law provisions.
Nothing in this Section shall prevent a party's ability to bring
cross-claims or counter-claims.

9. Miscellaneous
----------------

This License represents the complete agreement concerning the subject
matter hereof. If any provision of this License is held to be
unenforceable, such provision shall be reformed only to the extent
necessary to make it enforceable. Any law or regulation which provides
that the language of a contract shall be construed against the drafter
shall not be used to construe this License against a Contributor.

10. Versions of the License
---------------------------

10.1. New Versions

Mozilla Foundation is the license steward. Except as provided in Section
10.3, no one other than the license steward has the right to modify or
publish new versions of this License. Each version will be given a
distinguishing version number.

10.2. Effect of New Versions

You may distribute the Covered Software under the terms of the version
of the License under which You originally received the Covered Software,
or under the terms of any subsequent version published by the license
steward.

10.3. Modified Versions

If you create software not governed by this License, and you want to
create a new license for such software, you may create and use a
modified version of this License if you rename the license and remove
any references to the name of the license steward (except to note that
such modified license differs from this License).

10.4. Distributing Source Code Form that is Incompatible With Secondary
Licenses

If You choose to distribute Source Code Form that is Incompatible With
Secondary Licenses under the terms of this version of the License, the
notice described in Exhibit B of this License must be attached.

Exhibit A - Source Code Form License Notice
-------------------------------------------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

If it is not possible or desirable to put the notice in a particular
file, then You may include the notice in a location (such as a LICENSE
file in a relevant directory) where a recipient would be likely to look
for such a notice.

You may add additional accurate notices of copyright ownership.

Exhibit B - "Incompatible With Secondary Licenses" Notice
---------------------------------------------------------

  This Source Code Form is "Incompatible With Secondary Licenses", as
  defined by the Mozilla Public License, v. 2.0.
```

### BSD 3-Clause License

Each BSD crate's own file, since the holder and the name in clause 3 differ. alloc-no-stdlib 2.0.4:

```text
Copyright (c) 2016 Dropbox, Inc.
All rights reserved.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

subtle 2.6.1:

```text
Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved.
Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. 
```

tiny-skia 0.11.4:

```text
Copyright (c) 2011 Google Inc. All rights reserved.
Copyright (c) 2020 Yevhenii Reizner All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

  * Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in
    the documentation and/or other materials provided with the
    distribution.

  * Neither the name of the copyright holder nor the names of its
    contributors may be used to endorse or promote products derived
    from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

tiny-skia-path 0.11.4:

```text
Copyright (c) 2011 Google Inc. All rights reserved.
Copyright (c) 2020 Yevhenii Reizner All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

  * Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in
    the documentation and/or other materials provided with the
    distribution.

  * Neither the name of the copyright holder nor the names of its
    contributors may be used to endorse or promote products derived
    from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

tiny-skia-path 0.12.0:

```text
Copyright (c) 2011 Google Inc. All rights reserved.
Copyright (c) 2020 Yevhenii Reizner All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

  * Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in
    the documentation and/or other materials provided with the
    distribution.

  * Neither the name of the copyright holder nor the names of its
    contributors may be used to endorse or promote products derived
    from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

encoding_rs 0.8.42, for the data it derives from the WHATWG Encoding Standard:

```text
Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### BSD 2-Clause License

arrayref 0.3.9:

```text
Copyright (c) 2015 David Roundy <roundyd@physics.oregonstate.edu>
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the
   distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### ISC License

libloading 0.8.9:

```text
Copyright © 2015, Simonas Kazlauskas

Permission to use, copy, modify, and/or distribute this software for any purpose with or without
fee is hereby granted, provided that the above copyright notice and this permission notice appear
in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS
SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE
AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT,
NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.
```

rustls-webpki 0.103.15:

```text
Except as otherwise noted, this project is licensed under the following
(ISC-style) terms:

Copyright 2015 Brian Smith.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

The files under third-party/chromium are licensed as described in
third-party/chromium/LICENSE.
```

untrusted 0.9.0:

```text
// Copyright 2015-2016 Brian Smith.
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### zlib License

foldhash 0.2.0:

```text
Copyright (c) 2024 Orson Peters

This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

slotmap 1.1.1:

```text
Copyright (c) 2021 Orson Peters <orsonpeters@gmail.com>

This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

 1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

 2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

 3. This notice may not be removed or altered from any source distribution.
```

zlib-rs 0.6.8:

```text
(C) 2024 Trifecta Tech Foundation 

This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

### Boost Software License 1.0

Quoted from error-code 3.4.0's LICENSE. clipboard-win 5.4.1, by the same author, declares the same licence and carries no licence file of its own.

```text
Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the "Software") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third-parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the above license grant, this restriction and the following disclaimer,
must be included in all copies of the Software, in whole or in part, and
all derivative works of the Software, unless such copies or derivative
works are solely in the form of machine-executable object code generated by
a source language processor.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT
SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE
FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

### Community Data License Agreement, Permissive, 2.0

webpki-roots 1.0.9's LICENSE, for the root certificate data:

```text
# Community Data License Agreement - Permissive - Version 2.0

This is the Community Data License Agreement - Permissive, Version
2.0 (the "agreement"). Data Provider(s) and Data Recipient(s) agree
as follows:

## 1. Provision of the Data

1.1. A Data Recipient may use, modify, and share the Data made
available by Data Provider(s) under this agreement if that Data
Recipient follows the terms of this agreement.

1.2. This agreement does not impose any restriction on a Data
Recipient's use, modification, or sharing of any portions of the
Data that are in the public domain or that may be used, modified,
or shared under any other legal exception or limitation.

## 2. Conditions for Sharing Data

2.1. A Data Recipient may share Data, with or without modifications, so
long as the Data Recipient makes available the text of this agreement
with the shared Data.

## 3. No Restrictions on Results

3.1. This agreement does not impose any restriction or obligations
with respect to the use, modification, or sharing of Results.

## 4. No Warranty; Limitation of Liability

4.1. All Data Recipients receive the Data subject to the following
terms:

THE DATA IS PROVIDED ON AN "AS IS" BASIS, WITHOUT REPRESENTATIONS,
WARRANTIES OR CONDITIONS OF ANY KIND, EITHER EXPRESS OR IMPLIED
INCLUDING, WITHOUT LIMITATION, ANY WARRANTIES OR CONDITIONS OF TITLE,
NON-INFRINGEMENT, MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.

NO DATA PROVIDER SHALL HAVE ANY LIABILITY FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING
WITHOUT LIMITATION LOST PROFITS), HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE DATA OR RESULTS,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

## 5. Definitions

5.1. "Data" means the material received by a Data Recipient under
this agreement.

5.2. "Data Provider" means any person who is the source of Data
provided under this agreement and in reliance on a Data Recipient's
agreement to its terms.

5.3. "Data Recipient" means any person who receives Data directly
or indirectly from a Data Provider and agrees to the terms of this
agreement.

5.4. "Results" means any outcome obtained by computational analysis
of Data, including for example machine learning models and models'
insights.
```

### ring

ring 0.17.14 is in the window, through rustls, for the updater's and the
window's HTTPS. Its Cargo.toml declares `Apache-2.0 AND ISC`, and its
top-level LICENSE file explains the split:

```text
*ring* uses an "ISC" license, like BoringSSL used to use, for new code
files. See LICENSE-other-bits for the text of that license.

See LICENSE-BoringSSL for code that was sourced from BoringSSL under the
Apache 2.0 license. Some code that was sourced from BoringSSL under the ISC
license. In each case, the license info is at the top of the file.

See src/polyfill/once_cell/LICENSE-APACHE and src/polyfill/once_cell/LICENSE-MIT
for the license to code that was sourced from the once_cell project.
```

Its code from BoringSSL and OpenSSL is under the Apache License 2.0 (text
above); those files carry "Copyright The OpenSSL Project Authors" or
"Copyright The BoringSSL Authors" at their heads, and ring's LICENSE-BoringSSL
is the Apache License 2.0. Its fiat-crypto code (`third_party/fiat`) is
"Copyright 2015-2020 the fiat-crypto authors" under the Apache License 2.0.
The once_cell code it carries is under MIT or Apache-2.0 (texts above). Its
own new code is under this ISC licence, from LICENSE-other-bits:

```text
Copyright 2015-2025 Brian Smith.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### The fonts inside egui

egui, the window's toolkit, builds four fonts into every program that uses
it, through the crate epaint_default_fonts 0.36.2. Its Cargo.toml declares
`(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`: the crate's code is
MIT, and the fonts are under their own licences, quoted from the crate's
`fonts/` folder.

**Hack** (Hack-Regular.ttf). Copyright (c) 2018 Source Foundry Authors, and
Copyright (c) 2003 by Bitstream, Inc. for the Bitstream Vera glyphs it
builds on:

```text
The work in the Hack project is Copyright 2018 Source Foundry Authors and licensed under the MIT License

The work in the DejaVu project was committed to the public domain.

Bitstream Vera Sans Mono Copyright 2003 Bitstream Inc. and licensed under the Bitstream Vera License with Reserved Font Names "Bitstream" and "Vera"
MIT License

Copyright (c) 2018 Source Foundry Authors

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
BITSTREAM VERA LICENSE

Copyright (c) 2003 by Bitstream, Inc. All Rights Reserved. Bitstream Vera is a trademark of Bitstream, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy of the fonts accompanying this license ("Fonts") and associated documentation files (the "Font Software"), to reproduce and distribute the Font Software, including without limitation the rights to use, copy, merge, publish, distribute, and/or sell copies of the Font Software, and to permit persons to whom the Font Software is furnished to do so, subject to the following conditions:

The above copyright and trademark notices and this permission notice shall be included in all copies of one or more of the Font Software typefaces.

The Font Software may be modified, altered, or added to, and in particular the designs of glyphs or characters in the Fonts may be modified and additional glyphs or characters may be added to the Fonts, only if the fonts are renamed to names not containing either the words "Bitstream" or the word "Vera".

This License becomes null and void to the extent applicable to Fonts or Font Software that has been modified and is distributed under the "Bitstream Vera" names.

The Font Software may be sold as part of a larger software package but no copy of one or more of the Font Software typefaces may be sold by itself.

THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL BITSTREAM OR THE GNOME FOUNDATION BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER DEALINGS IN THE FONT SOFTWARE.

Except as contained in this notice, the names of Gnome, the Gnome Foundation, and Bitstream Inc., shall not be used in advertising or otherwise to promote the sale, use or other dealings in this Font Software without prior written authorization from the Gnome Foundation or Bitstream Inc., respectively. For further information, contact: fonts at gnome dot org.
```

**Noto Emoji** (NotoEmoji-Regular.ttf). Copyright 2013 Google Inc. All
Rights Reserved. Noto is a trademark of Google Inc. Under the SIL Open Font
License 1.1, below.

**Ubuntu Light** (Ubuntu-Light.ttf). Copyright 2011 Canonical Ltd. Ubuntu
and Canonical are registered trademarks of Canonical Ltd. Under the Ubuntu
Font Licence 1.0, below.

**emoji-icon-font** (emoji-icon-font.ttf):

```text
MIT License

Copyright (c) 2014 John Slegers

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

### SIL Open Font License 1.1

For Courier Prime, Literata, Inter and Noto Emoji. Quoted from
epaint_default_fonts 0.36.2's `fonts/OFL.txt`:

```text
This Font Software is licensed under the SIL Open Font License,
Version 1.1.

This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font
creation efforts of academic and linguistic communities, and to
provide a free and open framework in which fonts may be shared and
improved in partnership with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply to
any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software
components as distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to,
deleting, or substituting -- in part or in whole -- any of the
components of the Original Version, by changing formats or by porting
the Font Software to a new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed,
modify, redistribute, and sell modified and unmodified copies of the
Font Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components, in
Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the
corresponding Copyright Holder. This restriction only applies to the
primary font name as presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created using
the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

### Ubuntu Font Licence 1.0

For Ubuntu Light. Quoted from epaint_default_fonts 0.36.2's `fonts/UFL.txt`:

```text
-------------------------------
UBUNTU FONT LICENCE Version 1.0
-------------------------------

PREAMBLE
This licence allows the licensed fonts to be used, studied, modified and
redistributed freely. The fonts, including any derivative works, can be
bundled, embedded, and redistributed provided the terms of this licence
are met. The fonts and derivatives, however, cannot be released under
any other licence. The requirement for fonts to remain under this
licence does not require any document created using the fonts or their
derivatives to be published under this licence, as long as the primary
purpose of the document is not to be a vehicle for the distribution of
the fonts.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this licence and clearly marked as such. This may
include source files, build scripts and documentation.

"Original Version" refers to the collection of Font Software components
as received under this licence.

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to
a new environment.

"Copyright Holder(s)" refers to all individuals and companies who have a
copyright ownership of the Font Software.

"Substantially Changed" refers to Modified Versions which can be easily
identified as dissimilar to the Font Software by users of the Font
Software comparing the Original Version with the Modified Version.

To "Propagate" a work means to do anything with it that, without
permission, would make you directly or secondarily liable for
infringement under applicable copyright law, except executing it on a
computer or modifying a private copy. Propagation includes copying,
distribution (with or without modification and with or without charging
a redistribution fee), making available to the public, and in some
countries other activities as well.

PERMISSION & CONDITIONS
This licence does not grant any rights under trademark law and all such
rights are reserved.

Permission is hereby granted, free of charge, to any person obtaining a
copy of the Font Software, to propagate the Font Software, subject to
the below conditions:

1) Each copy of the Font Software must contain the above copyright
notice and this licence. These can be included either as stand-alone
text files, human-readable headers or in the appropriate machine-
readable metadata fields within text or binary files as long as those
fields can be easily viewed by the user.

2) The font name complies with the following:
(a) The Original Version must retain its name, unmodified.
(b) Modified Versions which are Substantially Changed must be renamed to
avoid use of the name of the Original Version or similar names entirely.
(c) Modified Versions which are not Substantially Changed must be
renamed to both (i) retain the name of the Original Version and (ii) add
additional naming elements to distinguish the Modified Version from the
Original Version. The name of such Modified Versions must be the name of
the Original Version, with "derivative X" where X represents the name of
the new work, appended to that name.

3) The name(s) of the Copyright Holder(s) and any contributor to the
Font Software shall not be used to promote, endorse or advertise any
Modified Version, except (i) as required by this licence, (ii) to
acknowledge the contribution(s) of the Copyright Holder(s) or (iii) with
their explicit written permission.

4) The Font Software, modified or unmodified, in part or in whole, must
be distributed entirely under this licence, and must not be distributed
under any other licence. The requirement for fonts to remain under this
licence does not affect any document created using the Font Software,
except any version of the Font Software extracted from a document
created using the Font Software may only be distributed under this
licence.

TERMINATION
This licence becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER
DEALINGS IN THE FONT SOFTWARE.
```
