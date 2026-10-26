# Fountain input: partial support

PDF is Method Reader's primary path. `.fountain` input is **partially supported**:
the render pipeline runs in full and 18 of the 20 formatting options apply
normally, but two do not, because they are consumed in
`rust/crates/sp-fountain/src/serialize.rs`, upstream of the `.fountain` itself, where the
pipeline's durable artifact begins. One piece of Fountain syntax also renders
differently than another tool would render it.

| What | On `.fountain` input |
| --- | --- |
| `contdMode` | **Not applied.** `(CONT'D)` is already written into the cue text. Asking to strip it emits a warning on the CLI and in `--json` rather than silently doing nothing; re-convert the PDF to change it. |
| `rejoinSplitDialogue` | **Not applicable.** It repairs speeches that a PDF's pagination split across pages. Fountain has no pages, so there is nothing to rejoin. |
| Forced sluglines (`.BLACK`) | **Rendered as mini-slugs.** Method Reader writes its own secondary sluglines with Fountain's dot-force, so it reads a dot-forced heading back the same way: a bold uppercase line inside the current scene, with no section break and no TOC entry. A dot-forced line that also opens like a real slugline (`.INT. …`, `.EST. …`) still becomes a full scene heading. |

The forced-slugline row is a cost, not a feature. Fountain has one forcing
character for headings and Method Reader needs it for the secondary sluglines a PDF
gives it, so a hand-written `.THE BRIDGE` loses its place in the table of
contents. If you want a scene, write it as one.

## The guards

Until 2026-10-21 a `.fountain` was taken on trust: handing one over was the
claim that it is a screenplay. Since the window accepts one by drop or by
Choose a script (Fountain in), a `.fountain` you choose is checked first, so
a stray text file does not quietly become a book:

| What | On a `.fountain` you choose |
| --- | --- |
| Not UTF-8 text | **Refused** (`unreadable`): "this script is not UTF-8 text. Save it as UTF-8 plain text and try again". Reading it anyway would fill the book with replacement characters. A UTF-8 byte order mark is fine. |
| Empty, or only white space | **Refused** (`unreadable`), `--force` or not: there is nothing to make a book from. |
| No scene heading and no dialogue | **Refused as not a screenplay** (`not-screenplay`), the PDF's own refusal in the PDF's own words. `--force`, or Convert anyway in the window, overrules it. A title page alone is not enough. |
| Dialogue but no scene heading | **Converted**, with the PDF's warning that the book has no scene navigation. |
| The scanned-PDF check | **Not applicable.** A text file has no text layer to be missing. |

A `.fountain` the library made (one in a script folder, beside its
`source.json`) is not checked again: it passed when its script came in, or
was converted anyway, and the window renders it again every time a setting
changes. The decision is in `rust/PARITY.md`, Decisions, "Fountain in".

## In the library

With `--library` (the window always passes it) a `.fountain` gets a script
folder of its own, as a PDF does, whose `source.json` names the `.fountain`
you chose. The folder keeps a copy of it beside the book, with the pictures
its `[[img: ...]]` notes name, so tuning and Get an editable script work from
the library without going back to your file. The copy is written fresh on
every conversion; your own file is never written.

None of the option and syntax rows above affect Method Reader's own use of
the path. The app's preview window re-renders a script it already converted,
where the options were settled when the PDF was read and the dot-forced lines
are Method Reader's own. They matter when you bring a `.fountain` from
another tool.
