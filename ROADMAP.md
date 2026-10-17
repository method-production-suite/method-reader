# Roadmap

Method Reader converts screenplay PDFs into e-books that hold their shape on an
e-reader, works offline, and runs on macOS, Windows and Linux. This page is
what comes next, roughly in order.

This is a small project maintained by one person, so treat dates as absent
rather than optimistic. Things move up the list when people ask for them:
see [Influencing this list](#influencing-this-list).

## Now

- **Method Reader's own AZW3 on a real Kindle.** Over USB, a Kindle now gets an
  AZW3 that Method Reader writes itself (beta), with nothing else installed, so
  the Kindle file takes milliseconds instead of a Calibre conversion. It
  opens whole in Calibre's own reader every time in our tests, but it has
  not yet been on a Kindle. The first device pass decides whether it stays
  beta: the checks are printed by `cargo xtask device-checklist`.
- **Pictures on the oldest Kindles.** Pictures from the script now reach
  the e-book (beta), and a designed title card becomes its cover. The one
  file that leaves them out is the MOBI, the file for the oldest Kindles:
  no one has opened one with pictures in it on a real device yet, and a
  failure there could stop the book opening at all. It gets them once a
  test book has been tried on one.
- **Copying to new Kindles over USB.** Every Kindle sold since late 2024
  connects the way a phone does (MTP) instead of showing up as a drive, so
  Method Reader cannot see one, and its owners are sent to email. Email cannot
  carry the Kindle file with the best layout, so a new Kindle never gets
  it today. The plan is a small helper that talks to the Kindle directly,
  on Mac, Windows and Linux. On a Mac it has to pause the system's photo
  import service for the length of the copy. A new Kindle is on order for
  testing.
- **A way to report problems without a GitHub account.** Asking for a device
  and saying whether a reader worked need none now: the window sends both.
  Telling us a script converted badly still means opening an issue, which
  excludes most of the people Method Reader is built for.
- **Keeping `pdfjs-dist` current.** It parses untrusted PDFs, so it is the
  dependency that matters most.

## Next

Deliberately unwritten. What readers trip over decides this section. If
you're using Method Reader, what annoys you is more useful than anything on this
page.

## Later

**A library: seeing the books you have made.** The window writes every
conversion into the library folder and offers no way to look at what is in
there: its tabs are Convert, Read and Settings. A library view would
list your books, reopen one without converting its PDF again, and be the
obvious home for each script's settings.

**Sides: reading one character's script.** Extract only the scenes a given
character appears in, or emphasise their lines throughout. Method Reader already
identifies every scene and every character cue while converting. Actors,
directors and anyone running a table read do this by hand today.

**Comparing drafts.** Show what changed between two revisions of a script,
scene by scene, instead of a page-by-page PDF diff that reflows into noise.
Method Reader already recognises revision marks and scene boundaries.

**A conversion report.** Page count, scene count, and how many lines each
character speaks, all computed during conversion and then thrown away.
Useful if you read scripts in volume.

**A guide inside the app.** The most useful support writing this project has
is the Send-to-Kindle setup, because Amazon silently discards documents from
an unapproved sender and gives you no error to search for. That text lives in
a Markdown file on GitHub, which is exactly where the person hitting the
problem is not. A short guide shown at the point each question comes up would
reach them.

**A running header that says which scene you're in.** On paper you keep your
place with your thumb and the shape of the page. Reflowed onto a screen both
go away. Method Reader already knows every scene boundary, so carrying the current
scene heading in a page header is mostly a formatting question. Sideloaded
Kindles ignore most header styling, and a header costs a line on a six-inch
screen, so it would be a setting, off by default.

**The screenplay typeface, everywhere.** Method Reader asks each device for
Courier Prime, the typeface of a printed script, but most devices don't carry
it and substitute their own typewriter font. Putting the font inside the
e-book (its open licence allows exactly this) would make scripts look the
same nearly everywhere. It would be a setting rather than always on: the
four font styles add about half a megabyte, Amazon's Send to Kindle strips
embedded fonts, and reMarkable hasn't been tried with them.

**Converting several scripts at once** in the window. The command line can
already be looped; the window can't.

**Right-click a PDF in your file manager** and send it to your e-reader
without opening Method Reader at all.

## Influencing this list

The two most useful things you can send:

**A device report.** Kobo, tolino and reMarkable support is written but has
never run on real hardware, and the Windows and Linux downloads have never
been installed by a person. Five minutes with one of those, either outcome,
is worth more than any feature request.

**What broke.** A script that converted badly tells us more than a script
that converted well. Please don't attach confidential material: a
description of where the text sat on the page is almost always enough, and
`cargo xtask make-fixture` shows how to build a small invented script that
reproduces a given shape.

[Open an issue.](https://github.com/method-production-suite/method-reader/issues/new/choose)
