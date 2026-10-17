//! Every help text, word for word as src/cli.ts printed it but for the
//! lines changed on purpose since (tests/usage.rs, `CHANGED_ON_PURPOSE`).
//! Generated from the TypeScript; tests/usage.rs holds it to the texts
//! read out of src/cli.ts the day it was deleted.

pub const USAGE: &str = r#"method-reader: screenplay PDF → reflowable EPUB3 (via Fountain)

Usage:
  method-reader <input.pdf | input.fountain> [options]

PDF is the primary path. .fountain input is PARTIALLY SUPPORTED: contdMode
and rejoinSplitDialogue are applied when a PDF is read, so they do not take
effect here (asking to strip (CONT'D) warns rather than failing silently),
and the scanned-PDF and not-a-screenplay guards are PDF-only. See
docs/fountain-input.md.

A script's saved settings are used by the conversion that finds them: if
<script>.method-reader.json sits beside the input (or in the script's library
folder, under --library), this run renders with it and says so on stderr.
--options/--options-json override it knob by knob. Write one with the
settings command. The first --library conversion of a PDF with none saves
the settings it started from as the script's own, so a later change to the
app defaults reaches only new scripts. Turn that off with
app-settings --set '{"keepScriptSettings":false}': scripts converted from
then on follow the app defaults until they are tuned, and a script that
already has saved settings keeps them.

Underneath those saved settings and any --options you pass, a conversion
starts from the format defaults you chose in the app, if you chose any, and
otherwise from Method Reader's own.

Pictures (beta) carried out of a PDF are written as PNGs in a <name>.images
folder beside the .fountain, and the .fountain names each one in an
[[img: ...]] note. The EPUB shows each picture where its note sits, and so
does a later conversion of that .fountain, which reads the folder and never
writes it. The one exception is a full-page title card the script opens
with: the EPUB makes it the book's cover instead of its first page, and
turning the promoteCoverImage option off keeps it on its page. The
includeImages option takes them all out of the book; the notes and the
folder stay. The .mobi is text-only for now: no pictures and no cover until
they have been tried on an older Kindle.

Options:
  -o, --output <file>    EPUB output path (default: <input>.epub). Its folder
                         must already exist, and so must --fountain's,
                         --preview-html's, --fdx's and --azw3's
  --library              write into the library folder instead of beside the
                         input: <library>/<stem>/<stem>.epub. The library is
                         <Documents>/Method Reader: ~/Documents on macOS and
                         Windows, and XDG_DOCUMENTS_DIR (else ~/Documents)
                         elsewhere; a folder chosen with method-reader app-settings
                         overrides that, and $METHOD_READER_LIBRARY overrides both
  --fountain <file>      Fountain output path, PDF input only. Default: the
                         EPUB's path ending in .fountain, so <input>.fountain,
                         or out/x.fountain for -o out/x.epub
  --no-fountain          skip the intermediate .fountain (and its picture folder)
  --title <text>         override detected title
  --author <text>        override detected author
  --force                convert even if it doesn't look like a screenplay
  --mobi                 also write a .mobi (for the oldest Kindles over USB)
  --azw3 <file>          also write an AZW3 for Kindles over USB (beta),
                         made by Method Reader from the EPUB, nothing else needed
  --preview-html <file>  also write the script as one self-contained HTML file
  --preview-inline       put that same HTML in the --json result (for the app)
  --fdx <file>           also write the script as a Final Draft file to keep
                         editing (beta). A reconstruction: revision marks
                         and exact page breaks do not carry over
  --options <file.json>  formatting options (see docs/formatting-options-log.md)
  --options-json <json>  the same options as one JSON argument (for the app)
  --json                 machine-readable result on stdout (for the app)
  --progress             emit NDJSON progress to STDERR while converting
  --debug                also write the classified elements beside the book,
                         as <book>.elements.json
  -h, --help             show this help
      --version          print the version and exit

Commands:
  method-reader devices [--quick] [--json]  list connected e-readers
  method-reader send <file> [--device <id>] [--json]
                                            send an existing file to one
  method-reader settings <file.fountain> [--set <json>] [--json]
                                            read/write a script's own settings
  method-reader export <file.epub> [--for epub|kindle|kfx|azw3|mobi|fountain|fdx] [--json]
                                            the file you would put on a reader,
                                            or the script to keep editing
  method-reader app-settings [--set <json>] [--json]
                                            where books land, and what new scripts start from
  method-reader reveal <file> [--json]      show a file in the system's file manager
  method-reader eject <volume> [--json]     safely remove a reader before unplugging it
                                            (beta on Windows and Linux)
  method-reader kfx-status [--json]         can this computer make KFX for a Kindle?
  method-reader kfx-install [--json]        add the KFX plugin to Calibre (online)
  method-reader routes <file.epub> [--quick] [--json]
                                            every way this book can leave, best first
  method-reader route <key> <file.epub> [--out <path>] [--json]
                                            send it to Apple Books, Amazon, Mail, or save a copy
  method-reader update-decision --offered <v> --current <v> [--json]
                                            should this update be offered? (offline)
  method-reader update-should-check [--opted-in] [--last-checked <ms>] [--json]
                                            may a check be made now? (offline)

A verb is only a verb when no file of that name exists: a script saved as
"devices" still converts, and "./devices" always means the file.
Each verb has its own --help.
"#;

pub const DEVICES_USAGE: &str = r#"method-reader devices: list every connected e-reader

Usage:
  method-reader devices [--quick] [--json]

Lists USB-mounted Kindle, Kobo and tolino volumes, plus a docked reMarkable if
its USB web interface answers. Nothing connected is an empty list, not an error.

Options:
  --quick                answer at once, without looking for a reMarkable
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const SEND_USAGE: &str = r#"method-reader send: send an existing file to a connected reader

Usage:
  method-reader send <file> [--device <id>] [--json]

send never converts: convert first, then send the output. A reMarkable accepts
only PDF and EPUB.

Options:
  --device <id>          which reader, as the id `method-reader devices` prints.
                         Optional with exactly one connected; required with
                         several
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const SETTINGS_USAGE: &str = r#"method-reader settings: this script's own formatting

Usage:
  method-reader settings <file.fountain> [--set <json>] [--json]

Reads the settings stored beside the script (<Stem>.method-reader.json). --set
overlays a partial JSON object on what is there and saves it; knobs you do
not mention keep their values. A script with no saved settings of its own
starts from the format defaults you chose in the app, if you chose any, and
otherwise from Method Reader's own.

Options:
  --set <json>           a partial FormatOptions object to overlay and save
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const EXPORT_USAGE: &str = r#"method-reader export: the file you would put on a reader

Usage:
  method-reader export <file.epub> [--for epub|kindle|kfx|azw3|mobi|fountain] [--fountain <f>] [--out <path>] [--json]
  method-reader export <file.epub> --for fdx --fountain <f> [--out <path>] [--json]   (beta)
  method-reader export <file.epub> --for <type> --check [--json]

export never sends: it produces (or reuses) the right file, and
`method-reader send` moves it. kindle is a KFX where Calibre, its KFX Output
plugin and Kindle Previewer are all installed, and otherwise an AZW3
(beta) that Method Reader writes itself, with nothing else needed. kfx, azw3
and mobi each make only that file: kfx refuses when this computer lacks
its tools, and mobi, for the oldest Kindles, is rebuilt from the
.fountain. fountain hands over the script itself; the copy --out writes leaves out Method Reader's own [[fmt: ...]]
and [[img: ...]] notes, while the .fountain beside the book keeps them.

fdx (beta) is the script itself, as a Final Draft file you can keep
editing (Fade In, WriterDuet, Highland and Celtx open it too). It is built
from the .fountain, so --fountain is required, and it is rebuilt on every
call. It is a reconstruction from the PDF: revision colors and marks, and
the exact page breaks, do not carry over.

Options:
  --for <type>           which file you want (default epub): epub, kindle,
                         kfx, azw3 (beta), mobi, fountain or fdx (beta)
  --fountain <file>      the script's .fountain: required by mobi, fountain
                         and fdx
  --options-json <json>  this script's settings, so a rebuild keeps them
  --out <path>           also copy the result to this absolute path (its
                         extension must match the file produced). An FDX
                         is written only there, not beside the book
  --check                build nothing: say which file this computer makes,
                         whether the one beside the book is current, what
                         builds it (builtBy: calibre or screepub), and when
                         the book was last written (bookDate, in ms)
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const APP_SETTINGS_USAGE: &str = r#"method-reader app-settings: where books land, and what new scripts start from

Usage:
  method-reader app-settings [--set <json>] [--json]

Reads the app-wide settings: the folder converted books are saved into, the
format defaults a new script starts from, and whether a converted PDF keeps
the settings it was made with. --set takes a JSON object with any of:
  libraryPath         a full path, or null to go back to the default folder
  formatDefaults      a FormatOptions object, or null to go back to
                      Method Reader's own
  keepScriptSettings  true (the default): the first --library conversion of
                      a PDF with no saved settings saves the ones it started
                      from as its own. false: it saves nothing, so the script
                      follows the format defaults until it is tuned. Either
                      way a script that already has saved settings keeps
                      them. null goes back to true.

formatDefaults REPLACES the stored defaults: any knob it leaves out goes
back to Method Reader's own, unlike method-reader settings --set, which moves only
the knobs it names and leaves the rest of a script's own tuning alone.

When METHOD_READER_LIBRARY is set, it wins over the chosen folder: books land
there no matter what libraryPath says.

Options:
  --set <json>           libraryPath, formatDefaults and/or
                         keepScriptSettings, see above
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const REVEAL_USAGE: &str = r#"method-reader reveal: show a file in the system's file manager

Usage:
  method-reader reveal <file> [--json]

<file> must be a full path to a file that already exists. On macOS, Finder
opens with the file itself selected. On Windows and Linux, the file's
folder opens instead.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const EJECT_USAGE: &str = r#"method-reader eject: safely remove a reader before unplugging it

Usage:
  method-reader eject <volume> [--json]

<volume> must be the full path of a reader that `method-reader devices` lists
right now, such as /Volumes/Kindle or E:\. Any other disk is refused.
On macOS this runs diskutil eject; on Linux, udisksctl unmount and then
power-off; on Windows, the shell's own Eject (beta on Windows and Linux:
it works in our tests, but has not been tried widely on real readers).

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const KFX_STATUS_USAGE: &str = r#"method-reader kfx-status: can this computer make KFX for a Kindle?

Usage:
  method-reader kfx-status [--json]

A Kindle gets its best rendering from a KFX file, and making one needs three
free tools: Calibre, Amazon's Kindle Previewer, and the KFX Output plugin
inside Calibre. This says which are installed, where to get the missing ones,
and what a Kindle gets until then. Installs nothing; works offline.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const KFX_INSTALL_USAGE: &str = r#"method-reader kfx-install: install the KFX plugin into Calibre

Usage:
  method-reader kfx-install [--json]

Downloads the current KFX Output plugin from Calibre's own plugin index and
installs it with Calibre's own installer, replacing any older copy. Needs
Calibre, and the internet. Kindle Previewer is not installed by this: it is
Amazon's, and `method-reader kfx-status` says where to get it. Refuses, before
touching Calibre, on a system Amazon makes no Kindle Previewer for: the
plugin would have nothing to drive there.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const ROUTES_USAGE: &str = r#"method-reader routes: every way this book can leave, best first

Usage:
  method-reader routes <file.epub> [--quick] [--json]

Lists every route: readers plugged in over USB, a docked reMarkable, Apple
Books, Amazon's Send to Kindle, email, and saving a copy. The route chosen
last time is marked, even while it cannot fire; otherwise the first one that
can. A route that cannot fire right now is still listed, with what would fix
it. Reads the app settings file, never writes it.

Looking for a reMarkable means asking its USB web interface, which takes a
second and a half to give up when no tablet is docked. --quick does not ask:
its reMarkable row says it is still checking, and a second call without
--quick fills it in. Readers that mount as a drive are found either way.

Options:
  --quick                answer at once, without looking for a reMarkable
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const ROUTE_USAGE: &str = r#"method-reader route: send a book to Apple Books, Amazon or Mail, or save a copy

Usage:
  method-reader route <key> <file.epub> [--out <path>] [--json]
  method-reader route kindle-email-setup [--json]

Performs one route `method-reader routes` lists and, when it works, remembers it
as the one to choose next time. Readers over USB and a docked reMarkable go
through `method-reader send` instead. A route that cannot fire here is refused
with what would fix it.

Keys:
  apple-books            add the book to Apple Books, which syncs it to an
                         iPhone and iPad (on a Mac)
  send-to-kindle         open Amazon's Send to Kindle app with the book, or
                         its web page and the book's folder
  email-to-kindle        open a Mail message with the book attached (on a
                         Mac, with Apple Mail as the default mail app). A
                         book over 50 MB is refused: Amazon drops it
  save-epub              save a copy of the EPUB at --out
  save-kindle            save a Kindle file (KFX, or an AZW3 that is beta)
                         at --out
  kindle-email-setup     open Amazon's settings page, where your Kindle's
                         email address and the approved senders are. Takes
                         no book, and is not remembered

Options:
  --out <path>           where a save writes the copy: an absolute path whose
                         extension matches the file (the two saves only)
  --fountain <file>      the script's .fountain (save-kindle only; the
                         Kindle file is made from the EPUB, so it is optional)
  --options-json <json>  this script's settings, so a rebuild keeps them
                         (save-kindle only)
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
"#;

pub const UPDATE_DECISION_USAGE: &str = r#"method-reader update-decision: should this update be offered?

Usage:
  method-reader update-decision --offered <version> --current <version> [--json]

Judges; never fetches. You supply the release that was found and the
build that is running, and get back whether to offer it. Works offline.

Not plain semver, on purpose: a build past a tag calls itself
0.6.0-1-g965cb10, which semver reads as OLDER than 0.6.0, so a semver
updater offers the tag and installs a downgrade. This does not.

A refusal is an ANSWER and exits 0 with its reason. Only bad usage
exits non-zero.

Options:
  --offered <version>  the release an update check found
  --current <version>  the build that is running
  --json               machine-readable result on stdout (for the app)
  -h, --help           show this help
"#;

pub const UPDATE_SHOULD_CHECK_USAGE: &str = r#"method-reader update-should-check: may a check be made right now?

Usage:
  method-reader update-should-check [--opted-in] [--last-checked <epoch-ms>] [--json]

Decides; never fetches. Without --opted-in the answer is always no:
update checks are off by default. With it, at most once a day, and a
clock set backwards reads as "checked recently", never as overdue.

Options:
  --opted-in               the user has switched update checks on
  --last-checked <ms>      when a check last ran, in epoch milliseconds
  --json                   machine-readable result on stdout (for the app)
  -h, --help               show this help
"#;
