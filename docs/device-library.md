# What's on the reader: the device library

The owner asked (2026-10-19) whether Method Reader could show what is on a
connected e-reader and let him add, remove and manage it. This is the design
of both halves: the engine's two commands and the window's list. Target:
1.0.0.

## What each reader allows

| Reader | How it is reached | List | Add | Remove |
| --- | --- | --- | --- | --- |
| Kindle that mounts (before late 2024) | its volume | `documents/`, all depths | yes, as today | yes |
| Kobo | its volume | the whole volume, but not dot folders | yes, as today | yes |
| tolino | its volume | `Books/`, all depths | yes, as today | yes |
| reMarkable | its USB web interface | not yet | yes, as today | no: the interface has no delete |
| Kindle over MTP (late 2024 on) | the MTP helper, in the Mac app only (beta) | later | yes, from the Mac app | later |

Listing and removing work only on a reader the engine itself lists right now,
matched on its resolved volume path, exactly as `eject` does. A path the
window passes in is never trusted on its own.

## What counts as a book

A regular file under the reader's books folder with an extension a reader
opens: `.epub` (and `.kepub.epub`), `.azw3`, `.azw`, `.mobi`, `.kfx`, `.pdf`,
`.txt`. Folders whose names start with a dot are skipped (`.kobo`,
`.adobe-digital-editions`, a Mac's `.Trashes`), as are a Kindle's `*.sdr`
folders, which hold a book's reading position and notes, not books. A link is
never followed.

Each book is listed with its path on the reader, its path inside the books
folder, its size and its format.

## Which ones are Method Reader's

A book on the reader is Method Reader's when a file in the library has the
same size and the same bytes. A name match alone is not enough: two scripts
can share a name, and a person can rename a file. So the answer is exact,
and a book converted again since it was sent reads as not matching, which is
the truth: the copy on the reader is the old one. The listing names the
library folder it matched.

## Removing a book

`device-remove <volume> <file>` removes one file, and only when all of these
hold:

- the volume is a reader the engine lists right now;
- the file's resolved path is inside that reader's books folder (never the
  volume root's other files on a Kindle or tolino, never a dot folder);
- no part of the path from the volume down to the file is a link;
- it is a regular file with a book extension.

Anything else is refused before anything is touched. The window asks first
and names the file; the engine does not ask, so the command line is
scriptable.

What else goes with the book:

- **Kindle:** the book's `<name>.sdr` folder beside it, which holds its
  reading position, highlights and notes. The Kindle's own delete removes it
  too, so this matches what a person expects; the answer lists it, and the
  window's question says the notes go with the book. The book goes first, so
  a failure never leaves a book without its notes; if the notes folder then
  will not go (the reader unplugged in between, say), the answer is
  `remove-failed` and says the book was already removed.
- **Kobo:** nothing else. Its database (`.kobo/KoboReader.sqlite`) is never
  written by Method Reader; the Kobo drops a missing book's entry itself when
  it next processes its content, after the reader is unplugged.
- **tolino:** nothing else.

## Adding and replacing

Adding is `send`, unchanged: it already replaces a same-named copy. In the
window, replacing is the send panel's own copy button for the script on
screen (convert it again, then copy it), and an open list looks again after
every send, so the new copy shows. A button that sends any book in the
library from the list is not built: the window has no library list to pick
from yet. Built on that recommendation, reversible.

## The commands

- `method-reader device-books <volume> [--json]`: the books on that reader,
  each with `path`, `name` (its path inside the books folder), `size`,
  `format`, and `library` (the matching library folder, when there is one).
- `method-reader device-remove <volume> <file> [--json]`: removes the book
  and its side files; answers `removed`, a list of what went.

Both hold to the CLI's `--json` contract and its error shape. New error
codes: `not-a-book` (the file is outside the books folder, a link, or not a
book) and `remove-failed` (the reader refused).

## The window

The send panel (`rust/crates/sp-app/src/app/device_library.rs`, its rules
and words in `src/model/device_library.rs`) offers "Books on your Kindle"
(or Kobo, or tolino) beside Other ways whenever such a reader is plugged in
as a drive, and under the cards when no reader is chosen yet. It wears the
Beta badge until the hands-on pass. Pressed, it opens the list under the
row (a disclosure: its expanded state is told to a screen reader):

- **Your scripts:** the books Method Reader made, each with its format, its
  size and the library folder it came from.
- **Other books:** everything else in the books folder.

Each book has "Remove…". It asks first, under the book: "Remove Field
Station.azw3 from your Kindle?", and on a Kindle adds that its reading
place, highlights and notes go with it. Remove sends `device-remove`; Cancel
takes nothing. The answer is said in a status line a screen reader hears,
and the list looks again. A reader unplugged takes its list with it. While a
book is going, the panel's copy buttons wait, and while a copy runs, the
Remove buttons wait.

The window calls the engine through its one `Engine` trait:
`Request::DeviceBooks` and `Request::DeviceRemove` stand for exactly the
argv above (`tests/argv_contract.rs`), and `tests/direct_engine.rs` runs
both through the real engine on a scratch Kindle.

## Tests, and what needs hands

Every test runs on a fake volume in a scratch folder with a fake device list,
as eject's do, so no test ever touches a real reader. A real Kindle, Kobo and
tolino pass, listing, removing and checking the reader afterwards, goes on
the roadmap's hands-on list.
