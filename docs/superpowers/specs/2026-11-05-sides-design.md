# Sides: one character's scenes as a book

2026-11-05. The owner's pick from Later (ROADMAP.md: "Sides: reading one
character's script"). Builds on the script's report
(docs/superpowers/specs/2026-11-04-conversion-report-design.md).

## The problem

An actor, a director or anyone running a table read wants only the scenes
one character is in. Today they cut those pages out by hand.

## The decision: a second book, made from the library `.fountain`

Sides are an EPUB made from the script's library `.fountain`, tokenized as
every book is and with the scenes the character is not in left out. The
`.fountain`, the script's book and its settings are not touched.

**A character is in a scene** when they have a cue in it, or when an action
line in it names them as a whole word, in capitals or capitalized (MARA or
Mara, for the cue MARA). An extra scene costs a page turn and a missing one
costs an actor their entrance, so a mention counts. Dialogue that mentions
them does not: that is someone talking about them. Built on that
recommendation and marked so in the roadmap. To reverse: count cues alone.

## 1. The engine and the command line: `method-reader library-sides <folder> <character>`

- **What it takes:** one script folder, checked as `library-report` checks
  it (`listed_script`), and a character the report names. Case does not
  matter and extensions are dropped (`mara (v.o.)` is MARA). Anyone with no
  cue in the script is refused as a usage error.
- **What stays:** the title page, and whole scenes: a scene runs from its
  heading to the next (a mini-slug does not end one, by sp-fdx's
  `is_scene`). What comes before the first heading is kept by the same test.
- **Page markers:** a kept scene that opens on a page whose `= pg N` marker
  sat in a dropped scene gets that marker again just before its heading, so
  the book still says which page of the script a reader is on.
- **Settings and pictures:** the script's own settings over the app
  defaults, as its book. A picture goes in only when a kept scene shows it.
- **What it makes:** `<folder>/sides/<stem> sides - <NAME>.epub`, made again
  each time, its book title `<script's title>: <NAME> sides`. The file name
  holds the script's stem so it can be told apart on a reader; anything but
  a letter, a digit, a space, a dash or a dot becomes `_`.
- **What it answers**, with `--json`:
  `{"ok": true, "folder", "name", "title", "scenes", "ofScenes", "epub"}`,
  where `name` is the character as the report spells them, `title` the
  book's, `scenes` how many scenes were kept and `ofScenes` how many the
  script has. Without `--json`:
  `sides for WREN: 2 of 3 scenes, <path>`.

## 2. The window: Sides in the report

- The report's table has a **Sides** column, with a **Make** button on each
  character's line.
- It sends one `library-sides <folder> --json -- <name>`, and the status line
  says "Sides for MARA: 2 of 3 scenes.". A **Show the file** button under the
  table then reveals the book.
- While sides are being made, the other Make buttons and the rows' Rename and
  Remove from library wait.

## Not in this slice

- Emphasising a character's lines throughout the full script.
- Sending sides from the window, and formats other than EPUB.
- A cover picture from before the first scene, unless that part is kept.
- Listing a script's sides in the Library row.

## Tests

- **sp-engine:** which scenes are kept (a cue, an action mention in both
  spellings, a word that only contains the name, dialogue that mentions
  them, a mini-slug inside a scene, the part before the first heading); the
  marker carried over; the refusals; the file made, its `dc:title`, the
  full script's files untouched; the file name for an awkward character.
- **sp-cli:** the verb end to end in the sandbox, both forms, `--help`.
- **sp-app:** the argv contract; Make sends exactly one `library-sides`
  and says so on the status line; Show the file reveals it; it waits while
  busy.
