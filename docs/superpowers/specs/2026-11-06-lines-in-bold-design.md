# Lines in bold: one character's speech marked through the whole script

2026-11-06. The second piece of Sides (ROADMAP.md, and
docs/superpowers/specs/2026-11-05-sides-design.md lists it as not in that
slice).

## The problem

An actor marks their lines with a highlighter so they can find them on the
page and still read the scene around them. Sides leave out the scenes they are
not in. This keeps every scene, with their lines standing out.

## The decision: a second book, with their speech in bold

The book is an EPUB of the whole script, made from the library `.fountain` with
the script's own settings, as sides are. The `.fountain`, the script's book and
its settings are not touched. The only thing written is
`<folder>/sides/<stem> lines - <NAME>.epub`.

**Bold is the mark.** Most e-readers show no colour, so a colour highlight would
vanish on them, and an underline already means something in a script's own
emphasis.

**Only the words spoken are bold.** The cue and any parenthetical stay as they
are: a cue is already easy to find, and a parenthetical is not a line to learn.
Every speech by that character counts, under any extension (MARA (V.O.) is
MARA), on either side of dual dialogue.

Built on that recommendation and marked so in the roadmap. To reverse: pick
another mark in sp-epub's `render_blocks`.

**The renderer wraps the line in `<strong>`.** Adding `**` markers to the tokens
would be simpler, but the book's emphasis pattern can't span a literal `*`, so
"2 * 3" would print its stars. A wrapper holds whatever the line says, and
italics and notes inside it keep working. No rule is added to the stylesheet, so
every other book's bytes are unchanged. `FormatOptions` stays as it was: this is
not a setting of a script, it is a different book, so the choice travels in
`BodyOptions`, as a question the engine answers about each cue. sp-epub doesn't
learn what a character is.

## 1. The verb

`method-reader library-lines <folder> <character> [--json]` makes the same checks
as `library-sides`: only what `library` would list, and only a character the
report names, in any case.

`--json` is one line with `folder`, `name`, `title`, `speeches` and `epub`. Plain,
it says `lines in bold for WREN: 2 speeches, <path>`. The book's title is
`<title>: <NAME> lines`.

## 2. The window

The report's table has a **Lines in bold** column, with a **Make** button on each
character's line, next to Sides. The status line says "MARA's lines in bold: 12
speeches.", and **Show the file** reveals the book. While it is being made, the
other Make buttons and the rows' Rename and Remove from library wait, as they do
for sides.

## Not in this slice

- Bold in the sides book itself.
- Sending this book from the window (built since:
  docs/superpowers/specs/2026-11-06-send-sides-design.md), and formats other
  than EPUB.
- Marking more than one character in a book.

## Tests

- **sp-epub:** only the chosen speaker's lines are wrapped, across several lines,
  dual dialogue and a literal `*`; with no question asked, the output is what it
  was.
- **sp-engine:** the book made, its title, every scene kept, the speech in bold
  and someone else's not, nothing else written, the refusals.
- **sp-cli:** the verb end to end in the sandbox, both forms, `--help`.
- **sp-app:** the argv contract; Make sends exactly one `library-lines`, says so
  and shows the file; the real engine makes the book.

Not tried on a device: bold on a Kindle after the AZW3 and KFX conversions is
expected, because both read the EPUB's own markup, but it has not been seen.
