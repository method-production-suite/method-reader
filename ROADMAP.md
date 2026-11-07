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
- **1.1.0: released** (2026-10-31, on the owner's yes). New Kindles over
  USB from the Mac app, .osf in, Method Reader's own AZW3 out of beta, and
  the zip fix (9.0.5). Its crate-age exceptions (the zip advisory, and
  clipboard-win 5.4.3 for v1.1.0 alone) have both come out, each crate
  now being 14 days old.
- **Before the next release, on the machine whose answers they are:** run
  `cargo xtask parity --accept route-help route-help-json` (from `rust/`,
  with that checkout's `CARGO_TARGET_DIR`). Those two rows of the
  screenplay fixture are frozen on another machine, so this Mac skips
  them, and they still hold the old save-kindle line that called the
  AZW3 beta (#109). Read each diff first: only that wording may change.
  Waived for 1.1.0 alone (the owner, 2026-10-31: "Yes, waive it"), so it
  is still due before the next release.

Built on the recommendation (reversible):

- **Sending a made book opens it as the open book** (2026-11-06, for Sides).
  Every route and the Kindle rule are then the send panel's own, with nothing
  written twice. It replaces the book open in the window, as opening a library
  row does. To reverse: a send panel of its own in the report.
- **A character's lines are marked in bold, spoken words only** (2026-11-06,
  for Lines in bold). Most e-readers show no colour, and a cue is already easy
  to find. To reverse: pick another mark in sp-epub's `render_blocks`.
- **A character is in a scene when a cue or an action line says so**
  (2026-11-05, for Sides). A scene is kept when the character has a cue in
  it or an action line names them as a whole word with a capital (MARA,
  Mara, McCoy), since an
  extra scene costs a page turn and a missing one costs an entrance.
  Dialogue that mentions them does not count. To reverse: count cues alone
  (sp-engine's `commands/library_sides.rs`, `is_in`).
- **A script's report is read from its `.fountain`** (2026-11-04), not
  from numbers kept at convert time, so every script in the library has
  one, including those brought in as a `.fountain` or an `.osf`, and it
  cannot disagree with the book. A speech is a cue; a character is the
  cue's name without its extensions, so MARA (V.O.) is MARA; words are
  those of the dialogue lines, parentheticals left out. To reverse: keep
  the parser's counts in source.json at convert time and read them there.
- **Renaming a script keeps a name; its files keep the PDF's** (2026-11-03).
  The Library tab's Rename keeps the new name in the script folder's
  source.json: the row is called by it, and every book made into the folder
  (the PDF converted again, or the book tuned or reopened) takes it as its
  title, unless `--title` gives another. Its book is made again at once.
  Nothing on disk is renamed: the folder and its files keep the PDF's name,
  which is how the next conversion of that PDF finds its folder, and the
  `.fountain` is not edited. An empty name goes back to the title page's.
  On the command line, `method-reader library-rename <folder> <name>`. To
  reverse: rename the folder and its files instead, which needs the folder
  lookup (sp-engine's `library.rs`, `folder_for`) to read the kept name
  too.
- **Removing a script from the library goes to the Trash** (2026-11-03).
  The Library tab's Remove from library, after asking, moves the script's
  whole folder (its `.fountain`, book and own settings) to the system Trash,
  the Recycle Bin on Windows, where it can be put back; nothing is deleted
  outright, since that folder can hold the only copy of the script and its
  settings. On the command line, `method-reader library-remove <folder>`.
  On Windows a library on a drive with no Recycle Bin (a network drive, most
  memory cards) is refused with nothing moved, since Windows would offer to
  delete it outright there instead; the window and the command line say
  "Recycle Bin" there wherever they say "Trash". To reverse: the engine's one trash call (sp-engine's
  `commands/library_remove.rs`, `into_trash`) becomes a removal.
- **New Kindles over USB, from the Mac app only in 1.1.0** (2026-10-29).
  On Windows the helper's path has never met a Kindle, only a phone, and on
  Linux an ordinary user needs a udev rule the packages do not install, so
  only the Mac app carries the helper. To reverse: build `sp-mtp` in
  release.yml's Windows and Linux legs and lay it beside the window in
  their packages, once a Kindle has copied on each.
- **The MTP route keeps its Beta badge** (2026-10-29). One Kindle has
  copied through it, on one Mac. To reverse: the window draws the badge
  for any route whose device says transport "mtp" (sp-app's send panel);
  drop that rule once more Kindles have copied.

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
  some other library book (the Library tab reopens a book; sending one
  from there is not built yet).
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

- **The library view** (the owner's pick, 2026-11-02). A Library tab lists
  the books in the library folder, newest first, with their pages, scenes
  and when each was converted, and Open reopens one without converting its
  PDF again, even when that PDF has moved or gone. On the command line,
  `method-reader library`. On main (design:
  docs/superpowers/specs/2026-11-02-library-view-design.md). Removing a
  script is built: Remove from library on each row asks first, then moves
  its folder to the Trash (`method-reader library-remove`; design:
  docs/superpowers/specs/2026-11-03-library-remove-design.md). Each row's
  settings are built too: the row says when a script has settings of its
  own, and its Settings button reopens it on the Settings tab (design:
  docs/superpowers/specs/2026-11-03-library-row-settings-design.md).
  Renaming a script is built: Rename on each row keeps a name the row and
  its books go by, its files untouched (`method-reader library-rename`;
  design: docs/superpowers/specs/2026-11-03-library-rename-design.md). The
  library view's planned slices are done.
- **A script's report** (the owner's pick, 2026-11-04). Report on each
  library row shows the script's pages, scenes, speeches and words of
  dialogue, then each character, most speeches first: their speeches,
  words, scenes, and the scene and page they first speak in. On the
  command line, `method-reader library-report <folder>`. Built (design:
  docs/superpowers/specs/2026-11-04-conversion-report-design.md).
- **Sides** (the owner's pick, 2026-11-05). In a script's report, Make on
  a character's line makes a book of the title page and only the scenes
  they are in, into the script's `sides` folder, and Show the file finds
  it. On the command line, `method-reader library-sides <folder>
  <character>`. Built (design:
  docs/superpowers/specs/2026-11-05-sides-design.md). The Lines in bold
  column makes a book of the whole script with that character's speech in
  bold, into the same folder; on the command line, `method-reader
  library-lines <folder> <character>` (design:
  docs/superpowers/specs/2026-11-06-lines-in-bold-design.md). Send to
  e-reader, beside Show the file, opens either book on the Result page with
  its send panel (design: docs/superpowers/specs/2026-11-06-send-sides-design.md).
- **Keeping the PDF reader's dependencies current.** The parser follows
  pdf.js 6.3.289 on purpose, so its numbers stay what the parser was tuned
  on, but the libraries that read untrusted PDFs and images are kept current
  under the 14-day crate-age check and `cargo deny`.

## Next

Deliberately unwritten. What readers trip over decides this section. If
you're using Method Reader, what annoys you is more useful than anything on this
page.

## Later

**Comparing drafts.** Show what changed between two revisions of a script,
scene by scene, instead of a page-by-page PDF diff that reflows into noise.
Method Reader already recognises revision marks and scene boundaries.

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
