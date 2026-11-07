# Sending sides from the window

2026-11-06. The last piece of Sides (ROADMAP.md, and
docs/superpowers/specs/2026-11-05-sides-design.md lists it as not in that
slice). It covers both books the report makes: sides and lines in bold.

## The problem

The report makes a book and Show the file finds it, and then the reader has to
get it onto their device by hand. That also bypasses the rule that a Kindle
never gets an EPUB over USB, since all they have is an EPUB.

## The decision: the book becomes the open book

**Send to e-reader**, beside Show the file, opens the book on the Result page,
where the send panel already is. Every route, the Kindle file and its rules, the
queue and the eject line are the ones a converted script gets. Nothing about
sending is written twice, and the Kindle rule holds because the panel enforces
it.

**A made book carries no `.fountain`.** Handing the panel the script's would be
wrong, not merely unneeded: a `.mobi`, `.fdx`, `.osf` or Fountain file is built
from it, so asking for one would give the whole script under the sides' name.
The panel already draws a book with no `.fountain`, those rows saying why they
are not available, and the AZW3 and KFX are made from the EPUB's own bytes. So the
book's Kindle file is the sides.

It replaces the book open in the window, as opening any other library row does.
The script stays in the Library, a click away.

Built on that recommendation and marked so in the roadmap. To reverse: a send
panel of its own in the report, which would mean a second home for what the
panel decides.

## What else that page reaches

- **Preview shows the book.** `library-sides` and `library-lines` answer with
  `previewHtml`, as a conversion does, with the same bold in it for lines. Without
  it, Preview would say the engine sent no pages.
- **Settings says where they are.** A made book takes its settings from its script
  and has none of its own. The page says so, in place of the fault a script whose
  settings can't be read gets, and offers no Convert it again.
- **The file goes beside the book.** A Kindle file lands in the `sides` folder
  next to its EPUB, as a script's does next to its own.

## Not in this slice

- Sending a book made in an earlier session: the Send button is on the report
  while the file it just made is known.
- Listing a script's sides in the Library row.

## Tests

- **sp-epub:** the preview takes the same bold.
- **sp-engine:** both answers carry the preview: the kept scenes only for sides,
  the bold for lines.
- **sp-app:** Send opens the Result page on that book, asks for routes on it, and
  a Kindle copy exports that book with no `.fountain`; Preview has pages; Settings
  has its line and no fault; Convert is left alone.

Not tried on a device.
