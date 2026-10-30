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

- **1.0.0: released** (2026-10-28, on the owner's yes). The first
  universal Mac build, the .fdx scene pages, the MCP server, what's on a
  reader, OSF out, the signed SHA256SUMS, and everything 0.13.1 would have
  carried (0.13.1 itself was skipped, the owner, 2026-10-26). Its draft
  passed every check before it was published, and 0.11.1, 0.13.0 and
  0.9.0 on Intel each install it.
- **The next release** tags on or after 2026-11-06, when clipboard-win
  5.4.3 is 14 days old: the owner's override for it held for v1.0.0
  alone, and the release check refuses it for any other tag until then.
  The zip fix (9.0.5) rides in it: 1.0.0 reads zips only with the one
  reader the advisory does not set against another, so there is no
  1.0.1 (the owner, 2026-10-29).

Built on the recommendation (reversible):

- **Intel Macs on 0.9.0 are offered 1.0.0** (2026-10-20). Every Intel
  copy of 0.9.0 reads the same update file, so the release now proves,
  before anything is published, that 0.9.0's own updater installs it under
  Rosetta from the Intel entry and that the app then starts and renames
  itself. Proved on this Mac with a throwaway key. The fallback, listing
  Intel only from 1.0.1, is a small change in the release tools.

- **What counts as a recent script** for the launcher's list: a conversion
  into the library, recorded under the file you chose. Opening a book that is
  already in the library, without converting it, does not count yet.
- **Replacing a book on a reader** is the send panel's own copy, after
  converting again; the list of what is on a reader has no button that sends
  some other library book (the window has no library list to pick from).
- **The checks a .fountain you bring has to pass** (Fountain in): it must be
  UTF-8 text, it must not be empty, and one with no scene heading and no
  dialogue is refused as not a screenplay, the same refusal a PDF gets, with
  the same Convert anyway. Until now a .fountain was taken on trust; with
  the window accepting one by drop, a stray text file would otherwise become
  a book. A .fountain the library made itself is still taken on trust, so
  tuning a script that was converted anyway keeps working.
- **What the library keeps for an .osf** (OSF in, 2026-10-27): the
  .fountain the OSF was read into, beside the book, which is what tuning
  and Get an editable script read; source.json names the .osf you chose.
  The .osf itself is not copied. Keeping it too is a small change.
- **How the OSF reader fills gaps** (OSF in, 2026-10-27): any style it does
  not know is action; a heading that does not open like INT., EXT. or EST.
  becomes a mini-slug, as in every .fountain Method Reader reads; and a
  title page with no bookmarks is read back by the writer's own layout
  (title, credit, author and source centered; contact, draft date and
  copyright at the foot). docs/osf-input.md has the rest.

Tests that need a device (hands only; everything else is automated):

1. Rebuild the proof sheet first: `cargo xtask make-fixture torture
   ../tests/fixtures/torture.pdf` (from `rust/`). A stale build once put an
   already-fixed defect in front of a reviewer.
2. Print the full list: `cargo xtask device-checklist` (from `rust/`). It
   says which file to build, which folder it goes in, and what to look for,
   item by item.
3. In order of what each answers:
   - **The new Kindle, page-foot test:** a cue left alone at the foot of
     a page, its speech on the next, in a KFX book. Every one seen sat on
     a cue that also carried a printed page number. Three test books
     (number in the cue, number under it, no numbers) are sent with the
     app and paged through, counting the cues left alone in each.
   - Done 2026-10-29, the new Kindle's first test: the MTP helper saw
     the Kindle, its one storage and its folders, copied Field Station's
     AZW3 over, and the Kindle opened it with every check passing
     (registry #20).
   - **A Kindle that mounts as a drive:** Method Reader's own AZW3
     and the pictures in a .mobi (the `mobi-picture-probe` books).
   - **A Kobo, a tolino, a reMarkable:** send Field Station and report
     either way (issues #26, #27, #28).
   - **The Mac after 1.0.0 arrives:** the app in Applications should be
     called Method Reader (it renames itself the first time 1.0.0 opens,
     from Screepub Desktop or Screepub). Check that Finder, Spotlight and
     Launchpad show the new name, and whether a Dock icon you kept still
     opens the app or shows a question mark.
   - **What's on a reader** (for 1.0.0): with a
     Kindle, a Kobo and a tolino, list the books, remove one Method Reader
     made, and check on the reader that it is gone, and on the Kindle that
     its notes went with it.
   - **Linux, eventually:** the owner's Arch check (AppImage, converting,
     USB detection, copy and eject on Hyprland, the command line); findings
     arrive as an issue. Nothing waits on it.

Deferred (the owner, 2026-10-20): **hands-on tests on Windows**, with no
date. The Windows build is still built and tested automatically on every
release and every Monday, and its notes keep saying it has not been
installed by a person yet.

Decided:

- **Security exceptions named by a GitHub advisory id** (the owner,
  2026-10-29: yes). While RustSec has no id for an advisory, its GHSA id
  makes a crate-age exception a security one, the zip 9.0.5 line
  included (SUITE-STANDARDS section 6).
- **The zip alert on the update proof** (the owner, 2026-10-29): dismissed
  as not shipped. The old updater it runs asks for zip 4 on Windows only,
  and the proof is built for the two Mac targets only.

- **Reporting a problem without a GitHub account** (the owner,
  2026-10-21: option 1). Help > Report a Bug, and Report a bug on a failed
  conversion, open a sheet that sends a pre-filled email to
  hello@methodproduction.com: what the person typed, the version, and for a
  failed conversion the app's own message. No words from the script; counts
  of its shape only if they tick a box. GitHub stays as a second link.
  Built (docs/superpowers/specs/2026-10-21-report-without-github-design.md).

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

- **An MCP server** (1.0.0). `method-reader mcp`: inspect a script, preview
  a page, get an editable script, make an e-book, all on local files. On
  main; it ships in 1.0.0.
- **OSF out.** A third editable script beside Fountain and .fdx, written from
  the same structure, and `method-reader export --for osf`. On main (beta);
  it ships in 1.0.0. Design and open questions:
  docs/superpowers/specs/2026-10-20-osf-out-design.md.
- **OSF and Fountain in.** The drop area and Choose a script accept .osf and
  .fountain, so a script from Method Storyteller becomes an e-book without a
  PDF. The Fountain half is built: a .fountain converts in the window and on
  the command line the way a PDF does, and the library keeps a copy of it
  beside its book. The OSF half is built too: an .osf is read into a
  .fountain and converts the same way, with the same checks
  (docs/osf-input.md). Neither has been tried yet on a script Method
  Storyteller itself wrote.
- **Copying to new Kindles over USB.** Every Kindle sold since late 2024
  connects the way a phone does (MTP) instead of showing up as a drive, so
  Method Reader cannot see one, and its owners are sent to email. Email
  cannot carry the Kindle file with the best layout. The plan is a small
  helper that talks to the Kindle directly. The owner has a new Kindle to
  test on (2026-10-28); the first test only reads what it reports when
  connected.
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
