# OSF out: a third editable script

2026-10-20. The ROADMAP item "OSF out": beside Fountain and .fdx, the
script as an Open Screenplay Format (OSF) document, written from the same
fountain-js tokens, as `method-reader export --for osf`.

## What OSF is, and where this comes from

OSF is an open XML format for screenplays. Its published description is the
Open Screenplay Format SDK (github.com/OpenScreenplayFormat/osf-sdk, moved
there from sourceforge.net/projects/openscrfmt). The SDK publishes, for each
version, an example document and the same document laid out as pages:

- v1.2: `file_format.xml` (and a numbered variant) and a C header of
  definitions;
- v2: `OSF-2.0.xml` and `OSF-2.0.pdf`, dated 2018-05-21 on its title page;
- v4: `OSF-4.0.xml` and `OSF-4.0.pdf`.

The SDK has no prose specification and no schema. Everything below is read
from those published example documents and from what their page layouts
show each element doing. Nothing here comes from any screenwriting app's
software or its files: this is a clean room (CLEAN-ROOM.md). Where the
examples do not settle a detail, this says so and takes the conservative
option, which is always the form the published 2.0 example itself uses.

Fade In is the app best known for reading and writing OSF, and the SDK's
example pages were laid out by it. The brief also named Trelby; whether
Trelby reads or writes OSF could not be confirmed from a public source, so
nothing here relies on it.

## The document

```xml
<?xml version="1.0" encoding="UTF-8"?>
<document type="Open Screenplay Format document" version="30">
  <paragraphs>
    <para>
      <style basestylename="Scene Heading"/>
      <text>INT. FIELD STATION - NIGHT</text>
    </para>
    ...
  </paragraphs>
  <titlepage>
    <para>
      <style basestylename="Normal Text" align="center"/>
      <text>FIELD STATION</text>
    </para>
    ...
  </titlepage>
</document>
```

- **The version it declares** is one constant, `OSF_VERSION` in
  `rust/crates/sp-osf/src/lib.rs`: `"30"`. The format calls itself version
  2.0, but the SDK's own 2.0 example declares `version="30"` (the 1.2
  example declares `"12"`, the 4.0 example `"40"`). So `"30"` is the value a
  reader of 2.0 documents has certainly been shown. It also matches the
  attribute spelling written here: the 1.2 and 2.0 examples name a
  paragraph's style with `basestylename`, while 4.0 renamed it `basestyle`.
  Should a reader turn out to want `"20"`, it is that one constant.
- **Minimal on purpose**, like the .fdx: the root, `<paragraphs>` and, when
  the script has one, `<titlepage>`. No `<info>`, `<settings>`, `<styles>`,
  `<spelling>` or `<lists>`. Those carry page size, margins, fonts, indents,
  numbering modes, revision colors and autocomplete lists; leaving them out
  lets the receiving app's own screenplay template decide how the page looks,
  the same choice the .fdx makes. Every paragraph names one of the format's
  built-in styles, which the examples number 0 to 7 (Normal Text, Scene
  Heading, Action, Character, Parenthetical, Dialogue, Transition, Shot), so
  nothing refers to a style the document would have to declare.
- **Layout follows the examples**: one `<para>` per paragraph, its
  `<style>` first and then its `<text>` runs, two-space indents. The
  elements between runs are markup, not text, so this indenting is safe in
  OSF (it is not in .fdx, where whitespace inside a paragraph is text).

## Element mapping

| Fountain token | OSF |
|---|---|
| scene heading | `<style basestylename="Scene Heading"/>` |
| scene heading that is a mini-slug (sp-fountain's `is_mini_slug`, e.g. LATER) | `Shot` |
| a scene number (`#12#`) | `scene_number="12"` on the heading's `<para>` |
| action | `Action` |
| centered (`>text<`) | `Action`, with `align="center"` on its `<style>` |
| character (extensions stay in the cue) | `Character` |
| parenthetical | `Parenthetical` |
| dialogue | `Dialogue` |
| lyrics | `Dialogue` inside a speech, `Action` outside one |
| transition (a forced `>` dropped) | `Transition` |
| page break (`===`) | `pagebreakbefore="1"` on the next paragraph's `<style>` |
| dual dialogue | both speeches in order, `dualdialogue="1"` on the first speech's `Character` style |
| notes `[[...]]`, Method Reader's own `[[fmt: ...]]` and `[[img: ...]]` | stripped (sp-fountain's notes.rs) |
| sections, synopses, boneyard, page markers | left out |
| title page | `<titlepage>`, see below |

- **Lines.** A token whose text holds several lines (a speech typed with
  hard returns, a lyric block) stays ONE paragraph, its lines joined by a
  newline inside the text: the 2.0 example shows exactly this as "a line
  with a soft return". Blank lines are dropped. Splitting a block into one
  paragraph per line, as the .fdx does, would be wrong here: Action's
  built-in style has a blank line before it, so a four-line action block
  would open up into four spaced paragraphs.
- **Emphasis.** Fountain's `*italic*`, `**bold**`, `***bold italic***`
  and `_underline_` become `<text>` runs with `bold="1"`, `italic="1"`,
  `underline="1"`, as in the 2.0 example's "Character-level formatting".
  The pairing is sp-fdx's `text_runs`, the one copy every editable script
  uses, so the .fdx and the .osf can never disagree about what is bold. A
  plain run carries no attribute. `\*` and `\_` are a literal asterisk and
  underscore.
- **Scene numbers.** The 2.0 example says a `scene_number` is used only
  when the document locks scene numbers, and is otherwise ignored. Locking
  is a `<settings>` attribute, which this writer does not write (a partial
  `<settings>` could reset the page size and margins it leaves out). So the
  numbers are carried, and an app that opens the file numbers scenes its
  own way until scene numbers are locked there. Only a Scene Heading
  carries one: whether a Shot may is not shown.
- **Dual dialogue.** Both published examples (1.2 and 2.0) mark the pair
  with `dualdialogue="1"` on the FIRST cue's style only, the second
  speech following as an ordinary Character and Dialogue. That is followed
  exactly. A page break before the exchange lands on that first cue.
- **Notes stripped.** OSF can carry a note as a `note` attribute on a
  paragraph, but notes are invisible by Fountain's spec and Method Reader's
  own notes are instructions to itself, so all of them are stripped, the
  same as in every other renderer.
- **Text XML cannot carry** (the control characters XML 1.0 forbids) is
  removed; `&`, `<`, `>` and `"` are escaped. Attribute values have tabs and
  line breaks turned into spaces.

## Title page

`<titlepage>` holds Normal Text paragraphs, like the 2.0 example's: blank
paragraphs (`<text></text>`) to bring the block a third of the way down,
then the title, credit, author(s) and source, centered (`align="center"`),
a blank paragraph between groups; then filler, and contact, draft date,
revision, copyright and notes at the foot, left-aligned. It is the .fdx's
layout and its constants (a 54-line page, 18 lines before the title), so
the two editable scripts open with the same page. Each title-page line is a
paragraph of its own (on a title page a line is a line). Note stripping
applies here too. A script with no title-page keys has no `<titlepage>`.

The 2.0 example also names title-page paragraphs with a `bookmark`
(Title, Author, Copyright, Draft, Contact). What a reader does with those
is not described, so they are left out.

## Format-option-free

Like the .fdx this is a reconstruction of the script, not a rendering of
the book: no FormatOptions knob changes it. It is rebuilt from the
.fountain on every call, so `--fountain` is required, and `--check` says
whether the file beside the book is newer than the .fountain.

## Where it is offered

- `method-reader export <book.epub> --for osf --fountain <f> [--out <path>]
  [--json]`, beta. Without `--out` it is written beside the book as
  `<book>.osf`. Its `--json` answer has the FDX answer's shape
  (`path`, `format: "osf"`, `extension: "osf"`, `label`, `stages`, then
  `available`, and `beta: true`).
- The window's Get an editable script cards (.fdx, .osf, Fountain), and
  the OSF row of Other ways > Save a file. Both run the same export.

## Left out

- `<info>` (uuid, page count), `<settings>`, `<styles>`, `<spelling>`,
  `<lists>` (characters, locations, transitions), header and footer
  styles: the receiving app supplies its own.
- Revision marks and colors (`revision`, `<marks>`), highlight colors,
  fonts and sizes per run, strikethrough, links, bookmarks, synopses and
  notes as attributes, columns, locked page numbers (`page_number`).
- Sections and synopses: OSF has no outline element in the examples.
- Pictures: a picture note is stripped like any other.

## Open questions

1. **The version value.** `"30"` is what the SDK's 2.0 example declares;
   whether any reader expects `"20"` for a 2.0 document is not shown
   anywhere public. One constant if it must change.
2. **Whether a reader needs `<styles>`, `<settings>` or `<info>`.** The
   examples always carry them; nothing says they are required. If an app
   refuses or misreads a file without them, the fallback is a `<styles>`
   block declaring only the eight built-ins by name and index.
3. **`basestylename` or `baseStyleName`.** A third-party parser's
   documentation says 2.0 and 2.1 spell it `baseStyleName`; the SDK's own
   2.0 example spells it `basestylename`. The SDK wins here.
4. **Dual dialogue's second cue.** Both examples mark only the first cue.
   Whether a reader also accepts (or needs) the mark on the second is not
   shown.
5. **Lyrics.** OSF shows no lyrics element. Dialogue inside a speech and
   Action outside one is a choice, not a mapping the format states.
6. **No app has opened one yet.** Like the .fdx, it is beta until a real
   app has opened field-station.osf and torture.osf and they read right.
