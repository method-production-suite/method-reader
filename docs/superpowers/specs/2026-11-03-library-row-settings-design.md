# A script's settings from its library row

2026-11-03. The library view's third slice (ROADMAP.md, Now: "Next: each
script's settings from its row"). Earlier slices:
docs/superpowers/specs/2026-11-02-library-view-design.md and
docs/superpowers/specs/2026-11-03-library-remove-design.md.

## The problem

Each script's own formatting lives in its library folder, and the Settings
tab tunes the script that is open. To change an older script's settings,
the person has to know that Open comes first and Settings second, and
nothing on the Library tab says which scripts have settings of their own.

## The slice

- **Each row says whether the script has its own settings.** When the
  engine's `settings` field names a file, the row's detail line ends with
  ", its own settings" ("18 pages, 11 scenes, converted 2 November 2026,
  its own settings"). Without one, the line is as before.
- **A Settings button on each row**, a quiet button beside Remove from
  library. It reopens the script exactly as Open does (one `convert-to`
  from the library `.fountain` to its `.epub`) and lands on the Settings
  tab instead of Preview. Its screen-reader name is "Settings for
  <title>". It is disabled whenever Open is.
- Nothing new in the engine: the `library` answer already carries
  `settings`, and the Settings tab already reads and saves the open
  script's settings.

## Windows, from MReview's audit of the removal (#131)

- **A drive with no Recycle Bin.** The Recycle Bin call Windows offers asks
  "delete permanently?" when the drive has no bin, and a yes deletes. So on
  Windows, before anything moves, `library-remove` checks that the library
  folder's drive has a `$Recycle.Bin` folder at its root, and refuses with
  "The library is on a drive with no Recycle Bin, so nothing was moved:
  <folder>" when it has none (a network drive, most memory cards). The
  check is a function of the folder and a "has a bin" probe, so it is
  tested on every system; the stand-in Trash skips it. The verb's help
  says so.
- **The word.** On Windows the window and the command line say "Recycle
  Bin" wherever they say "Trash": the button "Move to Recycle Bin", the
  question, the note, the status line and `moved <folder> to the Recycle
  Bin`. Every such word comes from one function of the platform, tested
  for both.
- **The seam.** `METHOD_READER_TRASH` works in a release build too, like
  the other stand-ins. docs/developers.md gains a short list of the
  environment variables that exist for tests only, this one among them.

## Tests

- **sp-app:** the detail line with and without settings; Settings on a row
  sends exactly one `convert-to` from the row's `.fountain` and lands on
  Settings; it waits while a conversion runs; its accessible name; every
  Trash word for both platforms.
- **sp-engine:** the drive check refuses with nothing moved when the probe
  says no bin, passes when it says yes, and is skipped for the stand-in.
- **sp-cli:** the plain-words line for both platforms.
