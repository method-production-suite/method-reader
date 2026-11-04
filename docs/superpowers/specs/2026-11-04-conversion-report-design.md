# A script's report

2026-11-04. The owner's pick from Later (ROADMAP.md: "A conversion
report. Page count, scene count, and how many lines each character
speaks, all computed during conversion and then thrown away").

## The problem

The parser knows every scene and every cue while it converts, and the
`--json` answer keeps only a count of characters and the five who speak
most. Someone reading scripts in volume (casting, coverage, a table read)
wants the whole picture of a script at a glance: how long it is, who
speaks, how much and where they first come in. Today they count by hand.

## The decision: the report reads the library `.fountain`

The report is made from the script's library `.fountain`, tokenized by
sp-fountain exactly as every book is made from it, not from numbers kept
at convert time. So it is there for every script in the library, the ones
converted before today, the ones brought in as a `.fountain` or an `.osf`,
and one whose `.fountain` was edited after converting, and it can never
disagree with the book. Pages and the title come from the same place the
Library row takes them (`script_in`): the count the PDF conversion kept
for this very `.fountain`, else its highest `= pg N` marker.

## 1. The engine and the command line: `method-reader library-report <folder>`

- **What it takes:** one script folder, as the `library` listing gives it,
  checked exactly as `library-remove` and `library-rename` check it
  (`listed_script`). It reads only: nothing is written.
- **What it counts,** from the `.fountain`'s tokens (title page left out):
  - **Scenes:** scene headings, mini-slugs counted out (sp-fdx's one
    definition of a Scene Heading, made public for this).
  - **A speech:** each cue (a `character` token), dual dialogue included.
  - **A character:** the cue's name with its notes and emphasis stripped,
    a forced cue's `@` and a dual cue's `^` dropped, and every trailing
    parenthetical dropped ("MARA (V.O.) (CONT'D)" is MARA), by sp-parser's
    `strip_extensions`, the one the parser names cues with. Two spellings
    that differ only in case are one character, under the first spelling
    seen.
  - **Words:** the words of each speech's `dialogue` lines (not its
    parentheticals), notes and emphasis stripped; a word is a run of
    non-space characters holding a letter or a digit.
  - **Scenes a character speaks in,** and the first: the scene number
    counts scene headings in order; a speech before the first heading is
    in no scene.
  - **The page a character first speaks on:** the label of the last
    `= pg N` marker before that cue, as printed. Before the first marker
    it is the page before the one that marker names (the serializer writes
    no marker for the page a script opens on), the rule the .fdx's scene
    pages already followed, now one function in sp-fountain's pages.rs
    (`opening_page`) that both ask. None without markers.
- **The order:** most speeches first; a tie goes to whoever spoke first.
- **What it answers**, with `--json`:
  `{"ok": true, "folder", "title", "pages", "scenes", "speeches", "words",
  "characters": [{"name", "speeches", "words", "scenes", "firstScene",
  "firstPage"}]}`, where `title` falls back to the folder's name as the
  row shows it, and `pages`, `firstScene` and `firstPage` are `null` when
  there is none.
- **Without `--json`:** the title, a line of totals, then one line per
  character:

  ```
  The Lamp Keeper
  3 pages, 2 scenes, 8 speeches, 23 words of dialogue
  WREN: 4 speeches, 15 words, 2 scenes, from scene 1, page 1
  ```

## 2. The window: Report on each library row

- A quiet **Report** button beside Settings, Rename and Remove from library.
- Pressing it sends one `library-report <folder> --json` and shows the
  answer under the row: one line of totals, then a table of characters
  (name, speeches, words, scenes, first scene, first page), with the
  table's header readable by a screen reader. Pressing it again (Hide
  report) closes it. One row's report is open at a time.
- It waits while a removal or rename of that row is under way; a refusal
  says so on the status line.

## Not in this slice

- A report of a PDF that is not in the library (convert it first).
- Saving or printing the report, scene lengths in eighths, lines per
  scene.

## Tests

- **sp-engine:** the counts on a small invented script: case-merged
  names, extensions, `@` and `^`, dual dialogue, a speech before the
  first heading, a mini-slug not counted, words without parentheticals or
  notes, the first page from markers, the order and its tie; the folder
  refusals; pages from source.json beside a `.fountain` without markers.
- **sp-cli:** the verb end to end in the sandbox, both forms, `--help`.
- **sp-app:** the argv contract; Report sends exactly one
  `library-report` and shows the totals and table; pressing again hides
  it; it waits while busy.
