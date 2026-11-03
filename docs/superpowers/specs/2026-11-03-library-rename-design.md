# Renaming a script in the library

2026-11-03. The library view's last planned slice (ROADMAP.md, Now:
"Next: renaming a script"). Earlier slices:
docs/superpowers/specs/2026-11-02-library-view-design.md,
docs/superpowers/specs/2026-11-03-library-remove-design.md and
docs/superpowers/specs/2026-11-03-library-row-settings-design.md.

## The problem

A row is called by its `.fountain` title page's Title, or, without one, by
the PDF's file name, which is often a draft's working name
("pinkrevs_v3_FINAL"). The book on the reader is called the same, or
"Untitled Screenplay" when there is no Title. There is no way to call a
script something else without editing its PDF or its `.fountain`.

## The decision: a kept name, the files untouched

Renaming keeps a name for the script in its folder's source.json, under
`"name"`. Nothing on disk is renamed: the folder and its files keep the
PDF's stem, because a conversion finds its folder again by that stem and
by the source source.json records, so renaming them would make the next
conversion of the same PDF a second script. The `.fountain` is not
edited either.

What the kept name changes:

- **The row:** the listing answers it as `title`, and the row shows it.
- **Every book made from the script:** a conversion into the library (from
  the PDF again, or from the library `.fountain` when a book is tuned or
  reopened) takes the kept name as the book's title, as `--title` would.
  A `--title` given on the command line still wins over it.
- **A reconversion keeps it:** converting the same PDF again does not drop
  the name.

Clearing the name (an empty one) goes back to the title page's Title.

Built on that recommendation and marked so in the roadmap's "Waiting on
the owner". To reverse: rename the folder and its files instead, which
needs the folder lookup to read the kept name too.

## 1. The engine and the command line: `method-reader library-rename <folder> <name>`

- **What it takes:** one script folder, as the `library` listing gives it,
  checked exactly as `library-remove` checks it (directly under the root,
  not a link, a folder the listing lists), and a name.
- **The name:** trimmed. Empty clears it. Longer than 200 characters, or
  holding a line break or another control character, is refused
  (a book's title is XML, which cannot carry them).
- **What it does:** keeps the name in source.json, then rebuilds the
  script's book (`<stem>.epub`) from its library `.fountain` with its own
  settings, so the file on disk is called by the new name at once. The
  other files beside it are not touched, and none of them is sent stale:
  the Kindle ladder reuses a `.azw3`, `.kfx` or `.mobi` only while it is
  newer than the book, so the next one asked for after a rename is made
  again (the `.azw3` and `.kfx` from the renamed book, the `.mobi` from
  the `.fountain`, which takes the kept name as the book does). A `.fdx`
  is the script itself, made from the `.fountain`, and keeps the title
  page's Title: renaming does not change it.
- **What it answers**, with `--json`:
  `{"ok": true, "folder": "<folder>", "title": "<name or the title page's>", "epub": "<path>"}`
  (with no Title on the title page, `title` is the folder's file name, as
  the row shows it). A name that starts with a dash goes after `--`
  (`library-rename <folder> --json -- <name>`), which is how the window
  always sends it.
  Without `--json`: `renamed <folder> to "<name>"`, or `<folder> is called
  by its title page again` when cleared.

## 2. The window: Rename on each row

- A quiet **Rename** button beside Settings and Remove from library.
- Pressing it puts a one-line text field under the row, holding the
  current name, with **Save** and **Cancel**; the keyboard goes to the
  field. Return saves, Escape cancels.
- Save sends one `library-rename`, then asks for `library` again, and the
  status line says "Renamed to <name>." or, when cleared, "<title> is
  called by its title page again."
- Disabled while a conversion runs or a removal or a rename is under way.
- When the renamed script is the one open, the window keeps it open; the
  next tune or send uses the new name, since the engine reads it.

## Not in this slice

- Renaming the files or the folder.
- Editing the author.

## Tests

- **sp-engine:** the name is kept and read; an empty name clears it; the
  refusals; a reconversion of the same PDF keeps it; a conversion from the
  library `.fountain` titles the book by it (the EPUB's `dc:title`); an
  explicit title still wins; the book is rebuilt.
- **sp-cli:** the verb end to end in a sandbox, both lines, `--help`.
- **sp-app:** the argv contract; Rename opens the field with the current
  name; Save sends exactly one `library-rename` and lists again; Cancel
  sends nothing; the status line; disabled while busy.
