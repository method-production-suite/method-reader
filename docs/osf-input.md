# OSF input

An `.osf` script (Open Screenplay Format, the XML some screenwriting apps
write, Method Storyteller among them) converts the way a `.fountain` does.
Drop it on the window, pick it with Choose a script, or pass it on the
command line:

```bash
method-reader "Field Station.osf" --library
```

Method Reader reads the OSF into a `.fountain` text (`sp_osf::osf_to_fountain`,
in `rust/crates/sp-osf/src/read.rs`) and from there it is a `.fountain` you
chose: the same guards, the same options, the same books. Everything in
[fountain-input.md](fountain-input.md) applies, including the two
formatting options that do not.

The reader is the writer's mapping turned round
(docs/superpowers/specs/2026-10-20-osf-out-design.md), read, like the
writer, only from the format's published example documents.

## The checks

| What | On an `.osf` you choose |
| --- | --- |
| Not UTF-8 | **Refused** (`unreadable`), in the `.fountain` refusal's words. |
| Not well-formed XML | **Refused** (`unreadable`): "this script is not a readable OSF file: it is not well-formed XML", with what the XML reader said. |
| XML with no `<paragraphs>` | **Refused** (`unreadable`): "this file is XML but not an OSF script: it has no <paragraphs>". |
| `<paragraphs>` with nothing in it | **Refused** as an empty script, `--force` or not. |
| No scene heading and no dialogue | **Refused as not a screenplay**, and Convert anyway (`--force`) overrules it, as for a `.fountain`. |

A document type declaration (DTD) is refused as not well-formed: the XML
reader does not expand entities from one.

## What it reads

| OSF | Fountain |
| --- | --- |
| `Scene Heading` | a scene heading; one that does not open like one (`INT.`, `EXT.`, `EST.`, `I/E`) is dot-forced, which Method Reader reads as a mini-slug |
| `scene_number` on a heading's `<para>` | `#n#` (letters, digits, `.`, `-` and `_` only) |
| `Shot` | a dot-forced mini-slug, as the writer's Shot came from one |
| `Action`, `Normal Text`, any style it does not know | action |
| `align="center"` | centered (`> text <`) |
| `Character`, then `Parenthetical` and `Dialogue` | one speech under an `@` cue; a parenthetical gets its brackets if it lacks them |
| `Parenthetical` or `Dialogue` with no cue above it | action |
| a cue with nothing under it | action |
| `dualdialogue="1"` on a cue | the next cue takes the `^`, so the two speeches are a pair (a document that marks both cues still makes one pair) |
| `pagebreakbefore="1"` | `===` before the paragraph |
| `Transition` | a forced transition (`> CUT TO:`), as Method Reader's own `.fountain` writes one |
| `<text>` runs with `bold`, `italic`, `underline` | `**`, `*`, `_`, with literal asterisks and underscores escaped where they would otherwise pair up |
| a soft return inside a paragraph | a new line in the same element |
| `<titlepage>` | the title-page keys, see below |

Styles are found by `basestylename` (the 1.2 and 2.0 spelling),
`basestyle` (4.0) or `baseStyleName`, by name in any case, or by the
built-ins' numbers 0 to 7. A style declared in `<styles>` is followed to the
built-in it is based on.

Each block is checked by reading it back alone: one that would come back as
something else (an action line in capitals that looks like a cue, one that
opens like a heading, `FADE OUT.`) is forced with `!`. A `/*` or `*/` in the
text is escaped, so it can never open Fountain's boneyard.

### The title page

A paragraph's `bookmark` (Title, Author, Copyright, Draft, Contact, as the
2.0 example names them) says which key it is; unmarked lines after it
belong to the same key until a blank paragraph. Without bookmarks the
writer's own layout is read back: the centered groups are the title, the
credit (when it reads like one, "Written by"), the author and the source,
in that order; the groups at the foot are a copyright (when it says so), a
draft date (when it reads like one) and the contact. Anything left over is
Notes.

### What it drops

Everything the writer leaves out, when another app wrote it: `<info>`,
`<settings>`, `<spelling>`, `<lists>`, revision marks and colors, notes,
fonts, sizes, highlight, strikethrough, links, synopses, sections and every
attribute not named above. A blank paragraph is dropped, as the writer
never writes one.

## The round trip

The committed invented scripts (Field Station, torture and the happy-path
fixture) go through the writer and back to the same tokens, less only what
the writer strips: notes, sections, synopses (the `= pg N` page markers
among them) and boneyard. A lyric comes back as dialogue inside a speech
and action outside one, because that is what the writer made of it.
`rust/crates/sp-osf/tests/read.rs` holds that.

## In the library

With `--library` (the window always passes it) the script gets its own
folder, and the folder keeps the `.fountain` the OSF was read into, beside
the book. Tuning and Get an editable script work from that `.fountain`,
the way they work for every other script; `source.json` names the `.osf`
you chose. Your `.osf` is never written. Outside the library only the book
is written beside it.

This is a decision built on the recommendation (ROADMAP.md, "Built on the
recommendation"): the `.fountain` is what every later step reads, so it is
the copy kept. Keeping the `.osf` itself as well would be a small change in
`rust/crates/sp-cli/src/convert.rs`.
