# Method Reader for developers

Method Reader is a three-stage pipeline: PDF to Fountain to EPUB3 (and MOBI). The
`.fountain` in the middle is kept as a durable, editable file. Every
formatting behaviour is an option (`rust/crates/sp-model/src/options.rs`),
set in the window's Settings page and on the command line with
`--options file.json`. The registry of options, with the reason for each, is
[`formatting-options-log.md`](formatting-options-log.md).

Setting up, testing and sending a change: [CONTRIBUTING.md](../CONTRIBUTING.md).

## Install the command-line converter

The converter ships on its own for macOS, Linux and Windows. Download the
file for your machine from the
[latest release](https://github.com/method-production-suite/method-reader/releases/latest),
unpack it, and run it.

| Machine | File |
| --- | --- |
| Mac, Apple silicon | `method-reader-cli-macos-arm64.tar.gz` |
| Linux, Intel or AMD | `method-reader-cli-linux-x64.tar.gz` |
| Linux, ARM (Asahi, Raspberry Pi, ARM servers) | `method-reader-cli-linux-arm64.tar.gz` |
| Windows, 64-bit | `method-reader-cli-windows-x64.zip` |

```bash
tar -xzf method-reader-cli-linux-x64.tar.gz
./method-reader script.pdf
```

There is no build for Intel Macs: Method Reader supports Macs with Apple
silicon only.

On a Mac with Apple silicon, Homebrew installs the same converter and keeps
it current:

```bash
brew install --formula ssandweiss/tap/method-reader
```

The Mac archive is signed and notarized. `SHA256SUMS` on the release page
covers the Linux and Windows archives. The Windows build is unsigned, so
SmartScreen warns the first time it runs: choose **More info**, then **Run
anyway**.

The window's own bundles (`.dmg`, `.exe`, `.deb`, `.rpm`) are listed in the
[README](../README.md#install). There is no Linux ARM package of the window;
`cargo xtask build-app-bundle --version <x.y.z> --out <dir> --arch arm64`,
run from `rust/` on an ARM machine, makes one by hand.

## The command line

```bash
method-reader <input.pdf | input.fountain> [options]
```

That is the installed converter. From a checkout, run the same thing from
`rust/` with `cargo run -p sp-cli --bin method-reader -- <input.pdf> [options]`;
every `method-reader` below works that way too.

`method-reader --help` is the full reference, and each verb has its own
`--help`. Some features are marked (beta): they work in our tests, but have
not been tried widely on real devices or apps yet, so tell us how they go.
They are the FDX, pictures in books, eject on Windows and Linux, and the
window's device requests and route reports. The options most people want:

| Option | Effect |
| --- | --- |
| `-o, --output <file>` | EPUB path (default `<input>.epub`; companions follow it) |
| `--library` | write into the library instead of beside the input (see below) |
| `--mobi` | also write a MOBI 6, for the oldest Kindles (text-only for now, see Pictures below) |
| `--azw3 <file>` | also write an AZW3 for Kindles over USB, made by Method Reader from the EPUB with nothing else installed (beta) |
| `--preview-html <file>` | also write the script as one self-contained HTML file |
| `--fdx <file>` | also write the script as a Final Draft file (beta; see [Final Draft (FDX)](#final-draft-fdx-beta)) |
| `--fountain <file>` / `--no-fountain` | where the intermediate `.fountain` goes, or skip it |
| `--options <file.json>` | formatting options (see the registry) |
| `--title` / `--author` | override detected metadata |
| `--force` | convert even if it doesn't look like a screenplay |
| `--json` | machine-readable result (the contract between the window and the engine) |
| `--progress` | NDJSON progress on **stderr** while converting: `{"progress":{"stage":...,"percent":N}}` ticks, plus one `{"progress":{"stage":"page","page":N,"paragraphs":[...]}}` line, sent once, for the first page that is not a title page, as plain paragraphs |
| `--debug` | dump classified elements as JSON |

### Pictures (beta)

A PDF's pictures (storyboard panels, photos, a designed title card) are kept
when they are art rather than page furniture, written as PNGs into a
`<stem>.images` folder beside the `.fountain`, and named there by
`[[img: ...]]` notes. The EPUB shows each one where it sat, and a title card
that opens the script becomes the book's cover; the `includeImages` and
`promoteCoverImage` options switch each off. AZW3 and KFX carry them, since
both are made from the EPUB. The engine's own `.mobi` does not yet:
it stays text-only until the pictures are tried on an older Kindle (registry
#19a). The `--json` answer gains `imagesPath` and `imageCount` when there are
pictures. The full rules are in the registry, entries #19, #19a and #19b.

A verb is only a verb when no file of that name exists, so a script saved as
`devices` still converts and `./devices` always means the file.

### Final Draft (FDX) (beta)

Method Reader can hand back the script itself, as a file to keep writing in,
not only a book to read. `--fdx <file>` on a conversion writes one beside
the other outputs, and `export` makes one from a book that is already
converted:

```bash
method-reader script.pdf --fdx script.fdx
method-reader export <file.epub> --for fdx --fountain <file.fountain> [--out <path>] [--json]
```

`export --for fdx` needs the script's `.fountain`, because that is what the
FDX is built from. Without `--out` it writes `<stem>.fdx` beside the book.
It is rebuilt on every call, and `--check` says whether the one beside the
book is newer than its `.fountain`.

The file opens in Final Draft, and Fade In, WriterDuet, Highland and Celtx
import it. It carries scene headings (with their scene numbers), action,
character cues with their extensions, parentheticals, dialogue, transitions,
lyrics, centered text, dual dialogue, forced page breaks, bold, italic and
underline, and a title page. Fountain notes (Method Reader's own formatting
and picture notes among them), sections, synopses and boneyard are left out,
so a picture carried out of the PDF never appears in the FDX. A
secondary heading such as LATER becomes a Shot, so it is not counted as a
scene. It names no fonts, margins or page layout, so the app that opens it
uses its own screenplay template.

It is a reconstruction from the PDF, so check it before you rely on it.
Revision colors and marks do not carry over, and neither do the exact page
breaks: the app lays the pages out again. The format version it declares is
one constant, `FDX_VERSION` in `rust/crates/sp-fdx/src/lib.rs`, in case an
older Final Draft refuses the file.

### Fountain input

`.fountain` input is partly supported: 18 of the 20 formatting options apply,
and one piece of syntax renders differently than another tool would render it.
See [Fountain input](fountain-input.md).

## The library

Without `--library` the CLI writes beside its input. With it (the window
always passes it), the book, its `.fountain` and its settings file go
together in one folder per script:

| Platform | Library |
| --- | --- |
| macOS | `~/Documents/Method Reader` |
| Windows | `%USERPROFILE%\Documents\Method Reader` |
| Linux / other | `<Documents>/Method Reader`, where `<Documents>` is `XDG_DOCUMENTS_DIR` (from the environment, else from `~/.config/user-dirs.dirs`), and `~/Documents` when neither says otherwise |

Each script gets `<library>/<stem>/<stem>.epub`. Two different PDFs with the
same name do not share a folder: the second gets `<stem>-<hash>`, keyed on its
own path. The `source.json` in each folder records which PDF it came from. A
`<stem>.method-reader.json` sitting beside the PDF from an earlier conversion is
copied in the first time that script reaches the library.

Books made by the older Mac app sit flat in the same default folder
(`<folder>/<stem>.epub`). The window leaves those books alone.

`method-reader app-settings --set '{"libraryPath": "/some/folder"}'` chooses a
different folder (a full path), and `'{"libraryPath": null}'` goes back to the
default. The window's Convert page offers the same choice: **Change…** beside
the library line, and **Reset**. Precedence is `METHOD_READER_LIBRARY`, then the
chosen folder, then the platform default. Choosing a new folder moves
nothing: books already converted stay where they are.

### Settings and defaults

A conversion starts from, in order of precedence: flags on the command line,
the script's own saved settings (`method-reader settings`), the app defaults, and
Method Reader's own defaults. `method-reader app-settings` reads or writes the app
defaults as `formatDefaults`; the window's Settings page has **Use these for
new scripts**, which makes that script's settings the app defaults.

The first `--library` conversion of a PDF with no settings of its own saves
the settings it started from as that script's own, so a later change to the
app defaults reaches only scripts converted afterwards. That is the default,
**Keep its settings**; **Follow the defaults** (or
`app-settings --set '{"keepScriptSettings": false}'`) saves nothing, so a new
script follows the app defaults until it is tuned.

`app-settings --set` also takes three keys the window remembers:
`readerDevice` (the reader the person said is theirs; default `null`, none),
`animatePage` (whether the first page types in; default `true`) and
`typewriterSounds` (whether the typewriter sounds play; default `false`).

`method-reader reveal <file>` shows a converted file in the system's file manager.

The app settings live in one small file outside the library:
`~/Library/Application Support/Method Reader/settings.json` on macOS,
`%APPDATA%\Method Reader\settings.json` on Windows, and
`$XDG_CONFIG_HOME/method-reader/settings.json` on Linux
(`~/.config/method-reader/settings.json` by default). `METHOD_READER_CONFIG_DIR`
overrides that folder everywhere.

## Devices and routes

```bash
method-reader devices [--quick] [--json]                # list connected e-readers
method-reader send <file> [--device <id>] [--json]      # send an existing file to one
method-reader export <file.epub> [--for epub|kindle|kfx|azw3|mobi|fountain|fdx]  # the file you would put on a reader
method-reader routes <file.epub> [--quick] [--json]     # every way this book can leave, best first
method-reader route <key> <file.epub> [--out <path>]    # Apple Books, Amazon, Mail, or save a copy
method-reader eject <volume> [--json]                   # safely remove a reader before unplugging
method-reader kfx-status [--json]                       # can this computer make KFX for a Kindle?
method-reader kfx-install [--json]                      # install the KFX plugin into Calibre (online)
```

`devices` lists USB-mounted Kindle, Kobo and tolino volumes, plus a
reMarkable if its USB web interface answers. `devices --quick` skips the
reMarkable probe, as `routes --quick` does. `send` delivers an existing file
and does not convert. For a mounted volume it copies the file where that
vendor indexes it; for a reMarkable it uploads over HTTP to the docked
tablet, which accepts only PDF and EPUB. With several readers connected,
`--device` is required.

`export --for` takes seven values:

- `epub`: the EPUB itself (the default).
- `kindle`: the Kindle file this computer makes: KFX where its tools are installed, and otherwise the AZW3 Method Reader writes itself (beta).
- `kfx`: a KFX file, and only that.
- `azw3` (beta): Method Reader's own AZW3, made from the EPUB with nothing else installed.
- `mobi`: the engine's own MOBI, for the oldest Kindles, and only that; the EPUB is left as it is. It needs `--fountain`.
- `fountain`: the script, for a screenwriting app. It needs `--fountain`, and that file must end in `.fountain`. With `--out`, the copy written there leaves out Method Reader's own `[[fmt: ...]]` and `[[img: ...]]` notes and keeps every other line as it is; the `.fountain` beside the book keeps them, because rebuilds read them, and an `--out` naming that file is refused while it carries one. Without `--out`, the answer names the `.fountain` beside the book, notes and all.
- `fdx` (beta): a Final Draft file rebuilt from the `.fountain` (see above).

`kfx` refuses, with an error, when this computer lacks the tools to make it;
`mobi` and `fountain` refuse without `--fountain`. The `--json` answer says
`"beta": true` for a file Method Reader still calls beta (the AZW3 and the FDX).
`export` never sends. It makes or reuses the file, and `send` moves it.

`eject <volume>` (beta on Windows and Linux) takes the full path of a reader that `devices` lists right
now, such as `/Volumes/Kindle` or `E:\`, and refuses any other disk. It runs
`diskutil eject` on macOS, `udisksctl unmount` then `power-off` on Linux (only
for a volume that is itself a mount point), and the shell's own Eject on
Windows (only for a whole drive, such as `E:\`, and then it waits up to five
seconds for the drive to go, because that verb reports success even when it
stays). A reader that will not eject (a file is open, say) fails with the code
`eject-failed`.

`routes` lists every other way out too (Apple Books, Amazon's Send to Kindle,
email, and saving a copy) with the one you chose last time marked. `route
<key>` performs one: `apple-books`, `send-to-kindle`, `email-to-kindle`,
`save-epub` or `save-kindle`. `route kindle-email-setup` opens Amazon's
Personal Document Settings page. The saves need an absolute `--out` path;
Apple Books and email are Mac only, and email needs Apple Mail as the default
mail app. Email refuses a book over 50 MB, Amazon's limit for email (it drops
a larger one without a word), and points to Send to Kindle, which takes up to
200 MB. `routes --quick` skips the second and a half spent looking for a
docked reMarkable.

A Kindle gets its best rendering from KFX, which needs Calibre, Amazon's
Kindle Previewer and the KFX Output plugin inside Calibre. `kfx-status` says
which are installed; `kfx-install` installs the plugin from Calibre's own
plugin index. Amazon makes no Kindle Previewer for Linux. Without all three,
a Kindle gets the AZW3 Method Reader writes itself (beta), which needs nothing
installed. KFX is optional; email and Send to Kindle need no tools at all and
get Amazon's own conversion.

Only the Kindle has been run on a real device, and only from a Mac. Kobo,
tolino and reMarkable are tested against injected mounts and a stub tablet;
no reader has been connected on Linux or Windows. See
[the verification ledger](verification-ledger.md).

## Architecture

One Cargo workspace, `rust/`, with a crate per stage:

```
rust/crates/
  sp-model            the shared data: parser elements, FormatOptions (the
                      single knob surface) and presets, fountain-js tokens
  sp-pdf              reads a PDF in pure Rust: text items, font facts, the
                      drawing operators and picture pixels the parser needs
  sp-parser           PDF facts to classified elements (geometry-driven:
                      elements are classified by where they sit on the page)
  sp-fountain         elements to Fountain (title block, CONT'D, dual
                      dialogue, styled text), the notes and slug rules every
                      renderer shares, and a fountain-js compatible tokenizer
  sp-epub             tokens to EPUB3 (options-driven CSS) and the preview
  sp-mobi             tokens to MOBI 6, and the PalmDB container
  sp-azw3             the EPUB to AZW3 (KF8), ported from Calibre's KF8
                      writer (beta)
  sp-fdx              tokens to Final Draft (.fdx) (beta)
  sp-fsutil           write-then-rename, the flushed replace, path rules
  sp-device           USB volumes, eject, reveal, the reMarkable web
                      interface, and MTP Kindles (beta, switched off)
  sp-mtp              method-reader-mtp, the MTP helper (off until it is tried on
                      hardware)
  sp-engine           conversion and its scanned and not-a-screenplay guards,
                      settings, the library, the Kindle ladder, Calibre for
                      KFX, the send routes and update decisions
  sp-cli              the method-reader command, and its --json contract
  sp-app              the window (egui, with AccessKit for screen readers)
  sp-update           the window's updater: latest.json, the minisign check,
                      the bundle swap and the restart
  sp-release          the release tools: bundle layouts, update archive and
                      signature, latest.json, Linux packages, the Windows
                      installer script
  sp-device-requests  the device-request service (a Cloudflare Worker)
  xtask               developer tools: `cargo xtask --help` lists them
site/                 methodreader.com, one static page with no build step
```

The window holds no logic of its own, and it has no engine process to run:
the engine is linked in. `sp-app` reaches it through one `Engine` trait
(`rust/crates/sp-app/src/engine.rs`) whose requests each stand for a
`method-reader ... --json` command line, and the answers keep the CLI's `--json`
shape, so the window and the command line answer every request with the
same code. See [the window's design](superpowers/specs/2026-10-11-sp-app-design.md);
[ADR 2026-09-12](adr/2026-09-12-cross-platform-tauri.md) is the history of
the Tauri window that came before it. Before changing
layout code, read
[screenplay-format-reference.md](screenplay-format-reference.md): it records
print geometry and what each e-reader's renderer honours.
