# Screepub for developers

Screepub is a three-stage pipeline: PDF to Fountain to EPUB3 (and MOBI). The
`.fountain` in the middle is kept as a durable, editable file. Every
formatting behaviour is an option (`src/options.ts`), set in the window's
Settings page and on the command line with `--options file.json`. The
registry of options, with the reason for each, is
[`formatting-options-log.md`](formatting-options-log.md).

Setting up, testing and sending a change: [CONTRIBUTING.md](../CONTRIBUTING.md).

## Install the command-line converter

The converter ships on its own for macOS, Linux and Windows. Download the
file for your machine from the
[latest release](https://github.com/ssandweiss/screepub/releases/latest),
unpack it, and run it.

| Machine | File |
| --- | --- |
| Mac, Apple Silicon | `screepub-cli-macos-arm64.tar.gz` |
| Mac, Intel | `screepub-cli-macos-x64.tar.gz` |
| Linux, Intel or AMD | `screepub-cli-linux-x64.tar.gz` |
| Linux, ARM (Asahi, Raspberry Pi, ARM servers) | `screepub-cli-linux-arm64.tar.gz` |
| Windows, 64-bit | `screepub-cli-windows-x64.zip` |

```bash
tar -xzf screepub-cli-linux-x64.tar.gz
./screepub script.pdf
```

On a Mac, Homebrew installs the same converter and keeps it current:

```bash
brew install --formula ssandweiss/tap/screepub
```

The Mac archives are signed and notarized. `SHA256SUMS` on the release page
covers the Linux and Windows archives. The Windows build is unsigned, so
SmartScreen warns the first time it runs: choose **More info**, then **Run
anyway**.

The window's own bundles (`.dmg`, `.exe`, `.deb`, `.rpm`) are listed in the
[README](../README.md#install). There is no Linux ARM package of the window;
`tools/build-app-bundle.ts` makes one by hand on an ARM machine.

## The command line

```bash
bun src/cli.ts <input.pdf | input.fountain> [options]
```

`bun src/cli.ts --help` is the full reference, and each verb has its own
`--help`. The options most people want:

| Option | Effect |
| --- | --- |
| `-o, --output <file>` | EPUB path (default `<input>.epub`; companions follow it) |
| `--library` | write into the library instead of beside the input (see below) |
| `--mobi` | also write a MOBI 6 (dependency-free USB sideload) |
| `--preview-html <file>` | also write the script as one self-contained HTML file |
| `--fdx <file>` | also write the script as a Final Draft file (see [Final Draft (FDX)](#final-draft-fdx)) |
| `--fountain <file>` / `--no-fountain` | where the intermediate `.fountain` goes, or skip it |
| `--options <file.json>` | formatting options (see the registry) |
| `--title` / `--author` | override detected metadata |
| `--force` | convert even if it doesn't look like a screenplay |
| `--json` | machine-readable result (the contract between the window and the engine) |
| `--progress` | NDJSON progress on **stderr** while converting |
| `--debug` | dump classified elements as JSON |

A verb is only a verb when no file of that name exists, so a script saved as
`devices` still converts and `./devices` always means the file.

### Final Draft (FDX)

Screepub can hand back the script itself, as a file to keep writing in,
not only a book to read. `--fdx <file>` on a conversion writes one beside
the other outputs, and `export` makes one from a book that is already
converted:

```bash
bun src/cli.ts script.pdf --fdx script.fdx
bun src/cli.ts export <file.epub> --for fdx --fountain <file.fountain> [--out <path>] [--json]
```

`export --for fdx` needs the script's `.fountain`, because that is what the
FDX is built from. Without `--out` it writes `<stem>.fdx` beside the book.
It is rebuilt on every call, and `--check` says whether the one beside the
book is newer than its `.fountain`.

The file opens in Final Draft, and Fade In, WriterDuet, Highland and Celtx
import it. It carries scene headings (with their scene numbers), action,
character cues with their extensions, parentheticals, dialogue, transitions,
lyrics, centered text, dual dialogue, forced page breaks, bold, italic and
underline, and a title page. Fountain notes (Screepub's own formatting
notes among them), sections, synopses and boneyard are left out. A
secondary heading such as LATER becomes a Shot, so it is not counted as a
scene. It names no fonts, margins or page layout, so the app that opens it
uses its own screenplay template.

It is a reconstruction from the PDF, so check it before you rely on it.
Revision colors and marks do not carry over, and neither do the exact page
breaks: the app lays the pages out again. The format version it declares is
one constant, `FDX_VERSION` in `src/fdx/fdx.ts`, in case an older Final
Draft refuses the file.

### Fountain input

`.fountain` input is partly supported: 16 of the 18 formatting options apply,
and one piece of syntax renders differently than another tool would render it.
See [Fountain input](fountain-input.md).

## The library

Without `--library` the CLI writes beside its input. With it (the window
always passes it), the book, its `.fountain` and its settings file go
together in one folder per script:

| Platform | Library |
| --- | --- |
| macOS | `~/Documents/Screepub` |
| Windows | `%USERPROFILE%\Documents\Screepub` |
| Linux / other | `<Documents>/Screepub`, where `<Documents>` is `XDG_DOCUMENTS_DIR` (from the environment, else from `~/.config/user-dirs.dirs`), and `~/Documents` when neither says otherwise |

Each script gets `<library>/<stem>/<stem>.epub`. Two different PDFs with the
same name do not share a folder: the second gets `<stem>-<hash>`, keyed on its
own path. The `source.json` in each folder records which PDF it came from. A
`<stem>.screepub.json` sitting beside the PDF from an earlier conversion is
copied in the first time that script reaches the library.

Books made by the older Mac app sit flat in the same default folder
(`<folder>/<stem>.epub`). The window leaves those books alone.

`screepub app-settings --set '{"libraryPath": "/some/folder"}'` chooses a
different folder (a full path), and `'{"libraryPath": null}'` goes back to the
default. The window's Convert page offers the same choice: **Change…** beside
the library line, and **Reset**. Precedence is `SCREEPUB_LIBRARY`, then the
chosen folder, then the platform default. Choosing a new folder moves
nothing: books already converted stay where they are.

### Settings and defaults

A conversion starts from, in order of precedence: flags on the command line,
the script's own saved settings (`screepub settings`), the app defaults, and
Screepub's own defaults. `screepub app-settings` reads or writes the app
defaults as `formatDefaults`; the window's Settings page has **Use these for
new scripts**, which makes that script's settings the app defaults.

The first `--library` conversion of a PDF with no settings of its own saves
the settings it started from as that script's own, so a later change to the
app defaults reaches only scripts converted afterwards. That is the default,
**Keep its settings**; **Follow the defaults** (or
`app-settings --set '{"keepScriptSettings": false}'`) saves nothing, so a new
script follows the app defaults until it is tuned.

`screepub reveal <file>` shows a converted file in the system's file manager.

The app settings live in one small file outside the library:
`~/Library/Application Support/Screepub/settings.json` on macOS,
`%APPDATA%\Screepub\settings.json` on Windows, and
`$XDG_CONFIG_HOME/screepub/settings.json` on Linux
(`~/.config/screepub/settings.json` by default). `SCREEPUB_CONFIG_DIR`
overrides that folder everywhere.

## Devices and routes

```bash
bun src/cli.ts devices [--json]                          # list connected e-readers
bun src/cli.ts send <file> [--device <id>] [--json]      # send an existing file to one
bun src/cli.ts export <file.epub> [--for kindle|epub|fdx] # the file you would put on a reader
bun src/cli.ts routes <file.epub> [--quick] [--json]     # every way this book can leave, best first
bun src/cli.ts route <key> <file.epub> [--out <path>]    # Apple Books, Amazon, Mail, or save a copy
bun src/cli.ts kfx-status [--json]                       # can this computer make KFX for a Kindle?
bun src/cli.ts kfx-install [--json]                      # install the KFX plugin into Calibre (online)
```

`devices` lists USB-mounted Kindle, Kobo and tolino volumes, plus a
reMarkable if its USB web interface answers. `send` delivers an existing file
and does not convert. For a mounted volume it copies the file where that
vendor indexes it; for a reMarkable it uploads over HTTP to the docked
tablet, which accepts only PDF and EPUB. With several readers connected,
`--device` is required.

`routes` lists every other way out too (Apple Books, Amazon's Send to Kindle,
email, and saving a copy) with the one you chose last time marked. `route
<key>` performs one: `apple-books`, `send-to-kindle`, `email-to-kindle`,
`save-epub` or `save-kindle`. `route kindle-email-setup` opens Amazon's
Personal Document Settings page. The saves need an absolute `--out` path;
Apple Books and email are Mac only, and email needs Apple Mail as the default
mail app. `routes --quick` skips the second and a half spent looking for a
docked reMarkable.

A Kindle gets its best rendering from KFX, which needs Calibre, Amazon's
Kindle Previewer and the KFX Output plugin inside Calibre. `kfx-status` says
which are installed; `kfx-install` installs the plugin from Calibre's own
plugin index. Amazon makes no Kindle Previewer for Linux. Without all three,
a Kindle gets AZW3 (with Calibre) or the engine's MOBI.

Only the Kindle has been run on a real device, and only from a Mac. Kobo,
tolino and reMarkable are tested against injected mounts and a stub tablet;
no reader has been connected on Linux or Windows. See
[the verification ledger](verification-ledger.md).

## Architecture

```
src/
  parser/     PDF to classified elements (geometry-driven: elements are
              classified by where they sit on the page)
  fountain/   elements to Fountain (title block, CONT'D, dual dialogue,
              styled text; slug.ts and notes.ts are shared by both renderers)
  epub/       fountain-js tokens to EPUB3 (jszip, options-driven CSS)
  mobi/       tokens to MOBI 6 (hand-built PalmDB container)
  device/     USB volumes and the reMarkable web interface
  export/     Calibre, KFX, and the send routes
  settings/   per-script settings and the app settings file
  options.ts  FormatOptions, the single knob surface
  convert.ts  orchestration and the scanned and not-a-screenplay guards
desktop/
  src-tauri/  the Rust window: runs the engine and shows its answer
  ui/         the window's pages, plain ES modules with no build step
site/         screepub.com, one static page with no build step
```

The window holds no logic of its own: it runs the engine with arguments and
draws the JSON the engine prints. See [desktop/README.md](../desktop/README.md)
and [ADR 2026-09-12](adr/2026-09-12-cross-platform-tauri.md). Before changing
layout code, read
[screenplay-format-reference.md](screenplay-format-reference.md): it records
print geometry and what each e-reader's renderer honours.
