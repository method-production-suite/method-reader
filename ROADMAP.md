# Roadmap

Method Reader converts screenplay PDFs into e-books that hold their shape on an
e-reader, works offline, and runs on macOS, Windows and Linux. This page is
what comes next, roughly in order.

This is a small project maintained by one person, so treat dates as absent
rather than optimistic. Things move up the list when people ask for them:
see [Influencing this list](#influencing-this-list).

## Waiting on the owner

Kept current for the Monday digest. Reversible decisions with a clear
recommendation are built on the recommendation and marked so here.

Release go-aheads:

- **0.13.1.** The clearer window wording, the Field Station sample on first
  launch, Linux app data moved to `~/.local/share`, Homebrew retired, one
  `SHA256SUMS` for every download, the shared updater, the list of recent
  scripts for the suite's launcher, on a Mac the app renaming itself Method
  Reader where an older install still calls it Screepub Desktop or Screepub
  and showing its own icon in the Dock, a long side-by-side exchange coming
  through as two speeches, several scripts copied to a Kindle in a row, and
  a tidier Other ways. The new release order (every build before any
  signing key) was proved on 2026-10-19 by a rehearsal of the release on
  main, with no tag. Three things before the tag:
  1. No young crate is left in the exception list. The earliest that holds
     is 2026-10-23, and the release workflow refuses an earlier tag on its
     own.
  2. Done 2026-10-20: the update is rehearsed from 0.13.0, headless,
     because 0.13.0's own updater is what installs 0.13.1. It installed
     0.13.1 with its signature verified.
  3. Done 2026-10-20: the rehearsal is green on the release commit,
     b0a211b. Since 2026-10-19 it is the only full build before a release.

  Then the tag goes to the owner through MOrg; one yes covers the tag and
  the publish, after the draft checks pass (docs/RELEASING.md).
- **1.0.0** (the owner, 2026-10-19). The next feature release after 0.13.1
  is 1.0.0, not 0.14: the .fdx scene pages, lengths and numbers, the
  command line's MCP server, what's on a reader, a SHA256SUMS signed with
  the update key, and the first universal Mac build (Intel and Apple
  silicon, the owner's decision of 2026-10-19). Checked ahead: every
  updater out there (0.12.x, 0.13.x) offers 1.0.0 as newer, the release
  tools and the version check take it, and the site's download links name
  no version. Launcher 0.1.2, which accepts a universal app, is out.

  One decision for the owner, before the universal build is written
  (docs/superpowers/specs/2026-10-20-universal-mac-design.md): every Intel
  copy of 0.9.0 reads the same update file, so once 1.0.0 lists Intel it is
  offered 1.0.0 straight from 0.9.0, a jump nothing has proved yet.
  Recommended: offer it, with a new rehearsal step in which 0.9.0's own
  updater installs the release and the app starts on 0.9.0's settings. The
  fallback, if that proof is slow to build: ship 1.0.0 universal but list
  Intel only from 1.0.1.

Built on the recommendation (reversible):

- **What counts as a recent script** for the launcher's list: a conversion
  into the library, recorded under the file you chose. Opening a book that is
  already in the library, without converting it, does not count yet.

Tests that need a device (hands only; everything else is automated):

1. Rebuild the proof sheet first: `cargo xtask make-fixture torture
   ../tests/fixtures/torture.pdf` (from `rust/`). A stale build once put an
   already-fixed defect in front of a reviewer.
2. Print the full list: `cargo xtask device-checklist` (from `rust/`). It
   says which file to build, which folder it goes in, and what to look for,
   item by item.
3. In order of what each answers:
   - **A Kindle that mounts as a drive:** Method Reader's own AZW3 (beta)
     and the pictures in a .mobi (the `mobi-picture-probe` books).
   - **A Kobo, a tolino, a reMarkable:** send Field Station and report
     either way (issues #26, #27, #28).
   - **The Mac after 0.13.1 arrives:** the app in Applications should be
     called Method Reader (it renames itself the first time 0.13.1 opens,
     from Screepub Desktop or Screepub). Check that Finder, Spotlight and
     Launchpad show the new name, and whether a Dock icon you kept still
     opens the app or shows a question mark.
   - **Linux, eventually:** the owner's Arch check (AppImage, converting,
     USB detection, copy and eject on Hyprland, the command line); findings
     arrive as an issue. Nothing waits on it.

Deferred (the owner, 2026-10-20): **hands-on tests on Windows**, with no
date. The Windows build is still built and tested automatically on every
release and every Monday, and its notes keep saying it has not been
installed by a person yet.

Decided:

- **The two Dependabot alerts for `glib`** (2026-10-20): dismissed as
  "vulnerable code not used". `glib` reaches the lock files only through
  Linux-only branches no shipped build compiles.

- **Window vocabulary** (2026-10-18): Preview for the in-app book, Send to
  e-reader for sending, Get an editable script for Fountain and .fdx, Taken
  from the PDF for the settings group; format cards named .fdx and Fountain.
  Merged.
- **Scripts as input** (2026-10-18): the window accepts .osf and .fountain as
  well as PDFs, in the same drop area, with the button renamed Choose a
  script.
- **CI** (2026-10-19): each change runs only the quick dependency checks on
  GitHub, after the full check on a Mac; everything is built and tested on
  macOS, Windows and Linux every Monday and before every release.
  Dependabot comes monthly, security fixes at once.
- **Homebrew** (2026-10-18): retired for good. Installs come from the
  downloads; the app updates itself.
- **Scene lengths in the .fdx** (2026-10-17, for 1.0.0): measured from the PDF
  when converting and kept beside the script, not in the .fountain. The
  rounding (nearest eighth, never under one eighth) is common practice and
  a best guess at Final Draft's own; nobody has compared the two on one
  script yet.

## Now

In order. Each is one small pull request.

- **Scene pages, lengths and numbers in the .fdx** (1.0.0). Built; its pull
  request opens after 0.13.1.
- **An MCP server** (1.0.0). `method-reader mcp`: inspect a script, preview
  a page, get an editable script, make an e-book, all on local files. Built
  and in review; it merges after 0.13.1.
- **OSF out.** A third editable script beside Fountain and .fdx, written from
  the same structure, and `method-reader export --for osf`.
- **OSF and Fountain in.** The drop area and Choose a script accept .osf and
  .fountain, so a script from Method Storyteller becomes an e-book without a
  PDF.
- **A way to report problems without a GitHub account.** Asking for a device
  and saying whether a reader worked need none now: the window sends both.
  Telling us a script converted badly still means opening an issue, which
  excludes most of the people Method Reader is built for.
- **Copying to new Kindles over USB.** Every Kindle sold since late 2024
  connects the way a phone does (MTP) instead of showing up as a drive, so
  Method Reader cannot see one, and its owners are sent to email. Email
  cannot carry the Kindle file with the best layout. The plan is a small
  helper that talks to the Kindle directly; it needs a new Kindle to test
  on.
- **Keeping the PDF reader's dependencies current.** The parser follows
  pdf.js 6.3.289 on purpose, so its numbers stay what the parser was tuned
  on, but the libraries that read untrusted PDFs and images are kept current
  under the 14-day crate-age check and `cargo deny`.

## Next

Deliberately unwritten. What readers trip over decides this section. If
you're using Method Reader, what annoys you is more useful than anything on this
page.

## Later

**A library: seeing the books you have made.** The window writes every
conversion into the library folder and offers no way to look at what is in
there: its tabs are Convert, Preview and Settings. A library view would
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
