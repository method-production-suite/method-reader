#!/usr/bin/env bun
// screepub — screenplay PDF → Fountain → reflowable EPUB3.
import { parseArgs } from 'node:util';
import { existsSync } from 'node:fs';
import { basename, delimiter, dirname, extname, join } from 'node:path';
import { readFile, writeFile, rename, rm } from 'node:fs/promises';
// Inlined by `bun build --compile`, so the shipped binary reports the same
// version as the tag that built it. release.sh checks the two agree.
import pkg from '../package.json' with { type: 'json' };
import {
  convertPdf,
  convertFountain,
  ScannedPdfError,
  NotAScreenplayError,
  type ConvertResult,
  type ConvertStage,
} from './convert';
import { mapConversionError, unwritableError, CliError, errorMessage, type JsonError } from './cli-errors';
// The picture folder beside a .fountain. Its policy (what may be read, what
// may be written, what may be pruned) lives there, not here, because nothing
// in cli.ts can be unit-tested: importing it runs main().
import {
  imageFolderWarnings,
  imagesDirFor,
  pruneImages,
  readImages,
  writeImages,
  type ImagesRead,
} from './library-images';
import { adoptSidecar, existingLibraryOutput, libraryOutput, libraryRoot } from './library';
import { resolveFormatOptions, type FormatOptions } from './options';
import { appDefaultOptions, keepsScriptSettings } from './settings/app-defaults';
import { readScriptSettings, saveScriptSettings, sidecarPath } from './settings/sidecar';
import { resolveCommand, devicesCommand, sendCommand, VERBS, type Verb } from './cli-devices';
import { updateDecisionCommand, updateShouldCheckCommand } from './cli-update';
import { settingsCommand } from './cli-settings';
import { exportCommand } from './cli-export';
import { appSettingsCommand } from './cli-app-settings';
import { revealCommand } from './cli-reveal';
import { kfxInstallCommand, kfxStatusCommand, installLines, setupLines } from './cli-kfx';
import { routeCommand, routesCommand, routesLines } from './cli-routes';
import { kfxPossible } from './export/kfx-setup';
import { routeFacts } from './export/route-facts';
import type { ListDevicesOptions } from './device/list';

const USAGE = `screepub: screenplay PDF → reflowable EPUB3 (via Fountain)

Usage:
  screepub <input.pdf | input.fountain> [options]

PDF is the primary path. .fountain input is PARTIALLY SUPPORTED: contdMode
and rejoinSplitDialogue are applied when a PDF is read, so they do not take
effect here (asking to strip (CONT'D) warns rather than failing silently),
and the scanned-PDF and not-a-screenplay guards are PDF-only. See
docs/fountain-input.md.

A script's saved settings are used by the conversion that finds them: if
<script>.screepub.json sits beside the input (or in the script's library
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
otherwise from Screepub's own.

Pictures carried out of a PDF are written as PNGs in a <name>.images folder
beside the .fountain, and the .fountain names each one in an [[img: ...]]
note. The EPUB shows each picture where its note sits, and so does a later
conversion of that .fountain, which reads the folder and never writes it.
The includeImages option takes them out of the book; the notes and the
folder stay. The .mobi does not show them yet.

Options:
  -o, --output <file>    EPUB output path (default: <input>.epub)
  --library              write into the library folder instead of beside the
                         input: <library>/<stem>/<stem>.epub. The library is
                         <Documents>/Screepub: ~/Documents on macOS and
                         Windows, and XDG_DOCUMENTS_DIR (else ~/Documents)
                         elsewhere; a folder chosen with screepub app-settings
                         overrides that, and $SCREEPUB_LIBRARY overrides both
  --fountain <file>      Fountain output path (default: <input>.fountain for PDF input)
  --no-fountain          skip the intermediate .fountain (and its picture folder)
  --title <text>         override detected title
  --author <text>        override detected author
  --force                convert even if it doesn't look like a screenplay
  --mobi                 also write a .mobi (for USB sideload to Kindle)
  --preview-html <file>  also write the script as one self-contained HTML file
  --preview-inline       put that same HTML in the --json result (for the app)
  --options <file.json>  formatting options (see docs/formatting-options-log.md)
  --options-json <json>  the same options as one JSON argument (for the app)
  --json                 machine-readable result on stdout (for the app)
  --progress             emit NDJSON progress to STDERR while converting
  --debug                also dump classified elements, and let pdf.js's
                         internal warnings through to stderr
  -h, --help             show this help
      --version          print the version and exit

Commands:
  screepub devices [--json]                 list connected e-readers
  screepub send <file> [--device <id>] [--json]
                                            send an existing file to one
  screepub settings <file.fountain> [--set <json>] [--json]
                                            read/write a script's own settings
  screepub export <file.epub> [--for kindle|epub] [--json]
                                            the file you would put on a reader
  screepub app-settings [--set <json>] [--json]
                                            where books land, and what new scripts start from
  screepub reveal <file> [--json]           show a file in the system's file manager
  screepub kfx-status [--json]              can this computer make KFX for a Kindle?
  screepub kfx-install [--json]             add the KFX plugin to Calibre (online)
  screepub routes <file.epub> [--quick] [--json]
                                            every way this book can leave, best first
  screepub route <key> <file.epub> [--out <path>] [--json]
                                            send it to Apple Books, Amazon, Mail, or save a copy
  screepub update-decision --offered <v> --current <v> [--json]
                                            should this update be offered? (offline)
  screepub update-should-check [--opted-in] [--last-checked <ms>] [--json]
                                            may a check be made now? (offline)

A verb is only a verb when no file of that name exists: a script saved as
"devices" still converts, and "./devices" always means the file.
Each verb has its own --help.
`;

// A verb's --help must advertise the verb's OWN flags. parseVerbArgs accepts
// --device, --json and -h and nothing else; printing the conversion usage here
// offered -o, --mobi, --options and --progress, every one of which the verb
// parser rejects as an unknown flag.
const DEVICES_USAGE = `screepub devices: list every connected e-reader

Usage:
  screepub devices [--json]

Lists USB-mounted Kindle, Kobo and tolino volumes, plus a docked reMarkable if
its USB web interface answers. Nothing connected is an empty list, not an error.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const SEND_USAGE = `screepub send: send an existing file to a connected reader

Usage:
  screepub send <file> [--device <id>] [--json]

send never converts: convert first, then send the output. A reMarkable accepts
only PDF and EPUB.

Options:
  --device <id>          which reader, as the id \`screepub devices\` prints.
                         Optional with exactly one connected; required with
                         several
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const SETTINGS_USAGE = `screepub settings: this script's own formatting

Usage:
  screepub settings <file.fountain> [--set <json>] [--json]

Reads the settings stored beside the script (<Stem>.screepub.json). --set
overlays a partial JSON object on what is there and saves it; knobs you do
not mention keep their values. A script with no saved settings of its own
starts from the format defaults you chose in the app, if you chose any, and
otherwise from Screepub's own.

Options:
  --set <json>           a partial FormatOptions object to overlay and save
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const EXPORT_USAGE = `screepub export: the file you would put on a reader

Usage:
  screepub export <file.epub> [--for kindle|epub] [--fountain <f>] [--out <path>] [--json]
  screepub export <file.epub> --for kindle --check [--json]

export never sends: it produces (or reuses) the right file, and
\`screepub send\` moves it. Kindle climbs KFX → AZW3 → MOBI, taking the best
rung this machine can reach.

Options:
  --for <kindle|epub>    which file you want (default epub)
  --fountain <file>      the script's .fountain, needed to rebuild a MOBI
  --options-json <json>  this script's settings, so a rebuild keeps them
  --out <path>           also copy the result to this absolute path (its
                         extension must match the file produced)
  --check                build nothing: say which file this computer makes,
                         whether the one beside the book is current, what
                         builds it (builtBy: calibre or screepub), and when
                         the book was last written (bookDate, in ms)
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const APP_SETTINGS_USAGE = `screepub app-settings: where books land, and what new scripts start from

Usage:
  screepub app-settings [--set <json>] [--json]

Reads the app-wide settings: the folder converted books are saved into, the
format defaults a new script starts from, and whether a converted PDF keeps
the settings it was made with. --set takes a JSON object with any of:
  libraryPath         a full path, or null to go back to the default folder
  formatDefaults      a FormatOptions object, or null to go back to
                      Screepub's own
  keepScriptSettings  true (the default): the first --library conversion of
                      a PDF with no saved settings saves the ones it started
                      from as its own. false: it saves nothing, so the script
                      follows the format defaults until it is tuned. Either
                      way a script that already has saved settings keeps
                      them. null goes back to true.

formatDefaults REPLACES the stored defaults: any knob it leaves out goes
back to Screepub's own, unlike screepub settings --set, which moves only
the knobs it names and leaves the rest of a script's own tuning alone.

When SCREEPUB_LIBRARY is set, it wins over the chosen folder: books land
there no matter what libraryPath says.

Options:
  --set <json>           libraryPath, formatDefaults and/or
                         keepScriptSettings, see above
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const REVEAL_USAGE = `screepub reveal: show a file in the system's file manager

Usage:
  screepub reveal <file> [--json]

<file> must be a full path to a file that already exists. On macOS, Finder
opens with the file itself selected. On Windows and Linux, the file's
folder opens instead.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const KFX_STATUS_USAGE = `screepub kfx-status: can this computer make KFX for a Kindle?

Usage:
  screepub kfx-status [--json]

A Kindle gets its best rendering from a KFX file, and making one needs three
free tools: Calibre, Amazon's Kindle Previewer, and the KFX Output plugin
inside Calibre. This says which are installed, where to get the missing ones,
and what a Kindle gets until then. Installs nothing; works offline.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const KFX_INSTALL_USAGE = `screepub kfx-install: install the KFX plugin into Calibre

Usage:
  screepub kfx-install [--json]

Downloads the current KFX Output plugin from Calibre's own plugin index and
installs it with Calibre's own installer, replacing any older copy. Needs
Calibre, and the internet. Kindle Previewer is not installed by this: it is
Amazon's, and \`screepub kfx-status\` says where to get it. Refuses, before
touching Calibre, on a system Amazon makes no Kindle Previewer for: the
plugin would have nothing to drive there.

Options:
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const ROUTES_USAGE = `screepub routes: every way this book can leave, best first

Usage:
  screepub routes <file.epub> [--quick] [--json]

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
`;

const ROUTE_USAGE = `screepub route: send a book to Apple Books, Amazon or Mail, or save a copy

Usage:
  screepub route <key> <file.epub> [--out <path>] [--json]
  screepub route kindle-email-setup [--json]

Performs one route \`screepub routes\` lists and, when it works, remembers it
as the one to choose next time. Readers over USB and a docked reMarkable go
through \`screepub send\` instead. A route that cannot fire here is refused
with what would fix it.

Keys:
  apple-books            add the book to Apple Books, which syncs it to an
                         iPhone and iPad (on a Mac)
  send-to-kindle         open Amazon's Send to Kindle app with the book, or
                         its web page and the book's folder
  email-to-kindle        open a Mail message with the book attached (on a
                         Mac, with Apple Mail as the default mail app)
  save-epub              save a copy of the EPUB at --out
  save-kindle            save a Kindle file (KFX, AZW3 or MOBI) at --out
  kindle-email-setup     open Amazon's settings page, where your Kindle's
                         email address and the approved senders are. Takes
                         no book, and is not remembered

Options:
  --out <path>           where a save writes the copy: an absolute path whose
                         extension matches the file (the two saves only)
  --fountain <file>      the script's .fountain, needed to rebuild a MOBI
                         (save-kindle only)
  --options-json <json>  this script's settings, so a rebuild keeps them
                         (save-kindle only)
  --json                 machine-readable result on stdout (for the app)
  -h, --help             show this help
`;

const UPDATE_DECISION_USAGE = `screepub update-decision: should this update be offered?

Usage:
  screepub update-decision --offered <version> --current <version> [--json]

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
`;

const UPDATE_SHOULD_CHECK_USAGE = `screepub update-should-check: may a check be made right now?

Usage:
  screepub update-should-check [--opted-in] [--last-checked <epoch-ms>] [--json]

Decides; never fetches. Without --opted-in the answer is always no:
update checks are off by default. With it, at most once a day, and a
clock set backwards reads as "checked recently", never as overdue.

Options:
  --opted-in               the user has switched update checks on
  --last-checked <ms>      when a check last ran, in epoch milliseconds
  --json                   machine-readable result on stdout (for the app)
  -h, --help               show this help
`;

function verbUsage(verb: Verb): string {
  if (verb === 'devices') return DEVICES_USAGE;
  if (verb === 'settings') return SETTINGS_USAGE;
  if (verb === 'export') return EXPORT_USAGE;
  if (verb === 'kfx-status') return KFX_STATUS_USAGE;
  if (verb === 'kfx-install') return KFX_INSTALL_USAGE;
  if (verb === 'routes') return ROUTES_USAGE;
  if (verb === 'route') return ROUTE_USAGE;
  if (verb === 'update-decision') return UPDATE_DECISION_USAGE;
  if (verb === 'update-should-check') return UPDATE_SHOULD_CHECK_USAGE;
  if (verb === 'app-settings') return APP_SETTINGS_USAGE;
  if (verb === 'reveal') return REVEAL_USAGE;
  return SEND_USAGE;
}

// --json is the app's only channel: EVERY exit in that mode must be one
// parseable JSON object on stdout. jsonMode is therefore pre-scanned from
// raw argv — parseArgs itself can throw (unknown flag) before it would
// have told us the mode.
let jsonMode = process.argv.includes('--json');

/// Write to a temp file then rename into place, so a reader (e.g. the app's
/// reader window mid-render) never observes a partially-written output.
/// A write or rename that fails takes the temp file with it (a failed
/// conversion used to leave a whole hidden EPUB in the library), and the
/// original error is what the caller sees.
async function writeFileAtomic(
  path: string,
  data: Uint8Array | string,
  enc?: BufferEncoding,
): Promise<void> {
  const tmp = join(dirname(path), `.${basename(path)}.${process.pid}.tmp`);
  try {
    await writeFile(tmp, data, enc);
    await rename(tmp, path);
  } catch (error) {
    await rm(tmp, { force: true }).catch(() => {});
    throw error;
  }
}

/** Print one line on stdout and WAIT for it to leave this process.
 *
 * `console.log` to a PIPE is buffered, and exiting does not wait for the
 * tail. Measured on this machine: the engine's own answer for a generated
 * 1,000-scene script reached a piped caller cut to exactly 262,144 or
 * 655,360 bytes — 64 KiB multiples, the pipe buffer — one run in four, with
 * no app and no Tauri anywhere, while the same run redirected to a FILE was
 * always whole. An answer under one pipe buffer never noticed; a
 * --preview-inline answer is 1.85-2.6 KB per page, so a 120-page script is
 * already several buffers deep, and the desktop window reads exactly this
 * way. Anything that can exceed 64 KiB goes through here.
 *
 * Deliberately NOT everything. `fail()`, `showHelp()`, `showVersion()`, the
 * catch-all and the verb handlers all still use `console.log`, because each
 * of them writes a bounded answer — an error object, a fixed usage screen, a
 * version string, a device list — that cannot approach one pipe buffer, and
 * a plain `console.log` keeps them synchronous and callable from anywhere,
 * including a `never`-returning exit path where there is no one to await.
 * The rule is about SIZE, not about stdout: route a writer through `sayLine`
 * the moment its output can grow with the script.
 *
 * The wait is UNCONDITIONAL, not `if (!write(...)) await drain`. The
 * conditional form is correct only while the runtime flushes a write that
 * stayed under the high-water mark before it exits — true of Bun today, and
 * exactly the sort of unstated assumption that produced this bug. Waiting
 * for the write's own callback depends on nothing: measured over 10 runs of
 * a small answer, both forms take the same time to the millisecond.
 */
async function sayLine(text: string): Promise<void> {
  await new Promise<void>((resolve) => {
    process.stdout.write(`${text}\n`, () => resolve());
  });
}

function fail(error: JsonError): never {
  if (jsonMode) {
    console.log(JSON.stringify({ ok: false, error }));
  } else {
    console.error(`screepub: ${error.message}`);
  }
  process.exit(1);
}

function showHelp(usage: string = USAGE): never {
  if (jsonMode) {
    console.log(JSON.stringify({ ok: true, usage }));
  } else {
    console.log(usage);
  }
  process.exit(0);
}

function showVersion(): never {
  if (jsonMode) {
    console.log(JSON.stringify({ ok: true, version: pkg.version }));
  } else {
    console.log(`screepub ${pkg.version}`);
  }
  process.exit(0);
}

/** The one thing that fixes `screepub --json devices`.
 *
 * resolveCommand looks at argv[0] and nothing else, deliberately and
 * permanently: a verb found after a flag is indistinguishable from that flag's
 * value (`--title send`). So flag-before-verb — how a shell alias or a spawn
 * wrapper commonly builds argv — reaches the CONVERSION path and used to die
 * on `unsupported input type ""`, which names neither the cause nor the cure.
 * Message-only: dispatch is untouched.
 *
 * The hint is withheld when a file of that name exists, because then the user
 * really did mean the file — that is the shadowing rule, and telling them to
 * run the verb instead would be wrong. */
function verbHint(input: string): string {
  if (extname(input) !== '') return '';
  if (!(VERBS as readonly string[]).includes(input)) return '';
  if (existsSync(input)) return '';
  return `. Did you mean \`screepub ${input}\`? The verb must come first`;
}

function parseCliArgs() {
  return parseArgs({
    args: process.argv.slice(2),
    allowPositionals: true,
    options: {
      output: { type: 'string', short: 'o' },
      library: { type: 'boolean', default: false },
      fountain: { type: 'string' },
      'no-fountain': { type: 'boolean', default: false },
      title: { type: 'string' },
      author: { type: 'string' },
      force: { type: 'boolean', default: false },
      mobi: { type: 'boolean', default: false },
      'preview-html': { type: 'string' },
      'preview-inline': { type: 'boolean', default: false },
      options: { type: 'string' },
      'options-json': { type: 'string' },
      json: { type: 'boolean', default: false },
      progress: { type: 'boolean', default: false },
      debug: { type: 'boolean', default: false },
      help: { type: 'boolean', short: 'h', default: false },
      version: { type: 'boolean', default: false },
    },
  });
}

/** Test seams, and a debugging hook for the Tauri shell: the mount roots to
 * scan and the reMarkable base URL. Read ONLY by the device commands — the
 * conversion path does not consult them. Unset means "the real thing". */
function deviceSeams(): ListDevicesOptions {
  const roots = process.env.SCREEPUB_VOLUME_ROOTS;
  const endpoint = process.env.SCREEPUB_REMARKABLE_ENDPOINT;
  return {
    roots: roots ? roots.split(delimiter).filter(Boolean) : undefined,
    remarkableEndpoint: endpoint || undefined,
  };
}

function parseVerbArgs(args: string[]) {
  return parseArgs({
    args,
    allowPositionals: true,
    options: {
      device: { type: 'string' },
      set: { type: 'string' },
      for: { type: 'string' },
      fountain: { type: 'string' },
      'options-json': { type: 'string' },
      out: { type: 'string' },
      offered: { type: 'string' },
      current: { type: 'string' },
      'opted-in': { type: 'boolean', default: false },
      'last-checked': { type: 'string' },
      quick: { type: 'boolean', default: false },
      check: { type: 'boolean', default: false },
      json: { type: 'boolean', default: false },
      help: { type: 'boolean', short: 'h', default: false },
    },
  });
}

type VerbValues = ReturnType<typeof parseVerbArgs>['values'];
/** Every flag in parseVerbArgs' schema but --json and --help, which every
 *  verb takes. */
type VerbFlag = `--${Exclude<Extract<keyof VerbValues, string>, 'json' | 'help'>}`;

/** The flags each verb acts on: the ONE place that says so.
 *
 * The verbs share ONE parser, so every flag any verb takes parses for all of
 * them, and each verb must refuse the ones it cannot act on: a flag that
 * silently does nothing teaches the caller it did something. That used to be
 * written verb by verb, and two verbs missed flags (settings took --device,
 * export took --set). Now a refusal is read off this table, and so is the
 * owner a refusal names, so the two cannot disagree. A new verb fails to
 * type-check until it has a row; tests/cli-verb-flags.test.ts runs every verb
 * against every flag. */
const VERB_FLAGS: Record<Verb, readonly VerbFlag[]> = {
  devices: [],
  send: ['--device'],
  settings: ['--set'],
  export: ['--for', '--fountain', '--options-json', '--out', '--check'],
  'update-decision': ['--offered', '--current'],
  'update-should-check': ['--opted-in', '--last-checked'],
  'kfx-status': [],
  'kfx-install': [],
  'app-settings': ['--set'],
  reveal: [],
  routes: ['--quick'],
  // For some keys only: routeCommand refuses each for every key that cannot
  // act on it.
  route: ['--out', '--fountain', '--options-json'],
};

/** The order a call carrying SEVERAL foreign flags hears about them in, kept
 * exactly as it was when each verb refused its own: the update verbs' flags
 * first, except on those two verbs, which name the other one's flags last. */
const UPDATE_FLAGS: readonly VerbFlag[] = ['--offered', '--current', '--last-checked', '--opted-in'];
const OTHER_FLAGS: readonly VerbFlag[] = [
  '--out', '--device', '--set', '--for', '--fountain', '--options-json', '--quick', '--check',
];

/** The refusal for the first flag given that `verb` does not act on, naming
 *  the verbs that do, in VERBS order; null when every flag given is the
 *  verb's own. */
function foreignFlagRefusal(verb: Verb, values: VerbValues): string | null {
  const isUpdate = verb === 'update-decision' || verb === 'update-should-check';
  const order = isUpdate ? [...OTHER_FLAGS, ...UPDATE_FLAGS] : [...UPDATE_FLAGS, ...OTHER_FLAGS];
  for (const flag of order) {
    const value = values[flag.slice(2) as keyof VerbValues];
    // --opted-in is a boolean that defaults to false: false is "not given".
    if (value === undefined || value === false || VERB_FLAGS[verb].includes(flag)) continue;
    const owners = VERBS.filter((v) => VERB_FLAGS[v].includes(flag)).join(' and ');
    const aside = verb === 'devices' && flag === '--device' ? ': it lists every reader' : '';
    return `${verb} takes no ${flag}${aside} (${flag} belongs to ${owners})`;
  }
  return null;
}

async function runVerb(verb: Verb, args: string[]): Promise<void> {
  let parsed: ReturnType<typeof parseVerbArgs>;
  try {
    parsed = parseVerbArgs(args);
  } catch (err) {
    fail({ code: 'usage', message: errorMessage(err) });
  }
  const { values, positionals } = parsed;
  jsonMode = values.json;

  if (values.help) {
    showHelp(verbUsage(verb));
  }

  try {
    // Every flag refusal, for every verb, before any positional check and
    // before any handler runs: for kfx-install that order is what keeps a
    // mistyped command off the network and out of the user's Calibre, and
    // for settings it is what keeps a --set beside a foreign flag from
    // writing. tests/cli-kfx.test.ts and tests/cli-routes.test.ts pin it.
    const refusal = foreignFlagRefusal(verb, values);
    if (refusal !== null) fail({ code: 'usage', message: refusal });

    if (verb === 'devices') {
      // Rejected rather than ignored, for the same reason a foreign flag is:
      // silently accepting an argument the command cannot act on teaches the
      // user it did something.
      if (positionals.length > 0) {
        fail({ code: 'usage', message: `devices takes no arguments (got "${positionals[0]}")` });
      }
      const { devices } = await devicesCommand(deviceSeams());
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, devices }));
        return;
      }
      if (devices.length === 0) {
        console.log('no devices connected');
        return;
      }
      for (const d of devices) console.log(`${d.name} (${d.kind}): ${d.id}`);
      return;
    }

    if (verb === 'update-decision' || verb === 'update-should-check') {
      if (positionals.length > 0) {
        fail({ code: 'usage', message: `${verb} takes no arguments (got "${positionals[0]}")` });
      }

      if (verb === 'update-decision') {
        let decision: ReturnType<typeof updateDecisionCommand>;
        try {
          decision = updateDecisionCommand({ offered: values.offered, current: values.current });
        } catch (err) {
          fail({ code: 'usage', message: errorMessage(err) });
        }
        if (jsonMode) {
          console.log(JSON.stringify({ ok: true, ...decision }));
          return;
        }
        // A refusal is an answer, not an error: exit 0 either way.
        console.log(decision.offer ? `offer ${decision.version}` : `no: ${decision.reason}`);
        return;
      }

      let answer: ReturnType<typeof updateShouldCheckCommand>;
      try {
        answer = updateShouldCheckCommand({
          optedIn: values['opted-in'],
          lastChecked: values['last-checked'],
          now: Date.now(),
        });
      } catch (err) {
        fail({ code: 'usage', message: errorMessage(err) });
      }
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...answer }));
        return;
      }
      console.log(answer.check ? 'check' : 'do not check');
      return;
    }

    if (verb === 'kfx-status' || verb === 'kfx-install') {
      // Every refusal comes BEFORE anything runs. For kfx-install that order
      // is the point: a mistyped command must not reach the network or the
      // user's Calibre. tests/cli-kfx.test.ts pins the order in this source.
      if (positionals.length > 0) {
        fail({ code: 'usage', message: `${verb} takes no arguments (got "${positionals[0]}")` });
      }

      if (verb === 'kfx-status') {
        const setup = await kfxStatusCommand();
        if (jsonMode) {
          console.log(JSON.stringify({ ok: true, ...setup }));
          return;
        }
        for (const line of setupLines(setup)) console.log(line);
        return;
      }

      // A person at a terminal waits several seconds for a download; say so
      // on stderr, where it cannot disturb the one JSON line on stdout. Only
      // where an install can run at all: elsewhere kfxInstallCommand refuses
      // before installing, and this line would stand over that refusal.
      if (!jsonMode && kfxPossible(process.platform)) {
        console.error("installing the KFX plugin from Calibre's plugin index...");
      }
      const installed = await kfxInstallCommand();
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...installed }));
        return;
      }
      for (const line of installLines(installed)) console.log(line);
      return;
    }

    if (verb === 'settings') {
      if (positionals.length !== 1) {
        fail({ code: 'usage', message: 'expected exactly one .fountain (see --help)' });
      }
      const result = settingsCommand({ fountain: positionals[0], set: values.set });
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...result }));
        return;
      }
      console.log(`settings for ${basename(positionals[0])}: ${result.sidecar}`);
      for (const [key, value] of Object.entries(result.settings)) {
        console.log(`  ${key}: ${value}`);
      }
      return;
    }

    if (verb === 'export') {
      if (positionals.length !== 1) {
        fail({ code: 'usage', message: 'expected exactly one .epub to export (see --help)' });
      }
      const result = await exportCommand({
        epub: positionals[0],
        for: values.for,
        fountain: values.fountain,
        optionsJson: values['options-json'],
        out: values.out,
        check: values.check,
      });
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...result }));
        return;
      }
      for (const stage of result.stages) console.log(`  ${stage}`);
      console.log(`${result.label}\n  ${result.path}`);
      if (result.fresh !== undefined) {
        console.log(result.fresh ? '  built, and current' : '  not built yet, or older than the book');
      }
      return;
    }

    if (verb === 'app-settings') {
      if (positionals.length > 0) {
        fail({ code: 'usage', message: `app-settings takes no arguments (got "${positionals[0]}")` });
      }
      const result = appSettingsCommand({ set: values.set });
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...result }));
        return;
      }
      const { library, customized } = result;
      const where = library.fromEnv
        ? 'set by SCREEPUB_LIBRARY'
        : library.chosen !== null
          ? 'the folder you chose'
          : 'the default folder';
      console.log(`books are saved in ${library.path} (${where})`);
      console.log(
        customized
          ? 'new scripts start from: your own defaults'
          : "new scripts start from: Screepub's defaults",
      );
      console.log(
        result.keepScriptSettings
          ? 'when a PDF is converted: keep its settings'
          : 'when a PDF is converted: follow the defaults',
      );
      return;
    }

    if (verb === 'reveal') {
      if (positionals.length !== 1) {
        fail({ code: 'usage', message: 'expected exactly one file to reveal (see --help)' });
      }
      const result = await revealCommand(positionals[0]);
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...result }));
      }
      // Human mode: nothing on success. There is nothing left to say once
      // the file manager is the thing showing the answer.
      return;
    }

    if (verb === 'routes') {
      if (positionals.length !== 1) {
        fail({ code: 'usage', message: 'expected exactly one .epub (see --help)' });
      }
      // --quick leaves the reMarkable probe out, and says so on its row:
      // the Send page draws this at once, then asks again in full.
      const answer = await routesCommand(positionals[0], {
        facts: () => routeFacts({}, { ...deviceSeams(), skipRemarkable: values.quick }),
      });
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...answer }));
        return;
      }
      for (const line of routesLines(answer)) console.log(line);
      return;
    }

    if (verb === 'route') {
      // --out, --fountain and --options-json are route's own, and
      // routeCommand refuses each for every key that cannot act on it,
      // before it probes or opens anything.
      if (positionals.length < 1 || positionals.length > 2) {
        fail({ code: 'usage', message: 'expected a route key and one .epub (see --help)' });
      }
      const answer = await routeCommand({
        key: positionals[0],
        epub: positionals[1],
        out: values.out,
        fountain: values.fountain,
        optionsJson: values['options-json'],
      });
      if (jsonMode) {
        console.log(JSON.stringify({ ok: true, ...answer }));
        return;
      }
      console.log(answer.note);
      return;
    }

    // verb === 'send'
    if (positionals.length !== 1) {
      fail({ code: 'usage', message: 'expected exactly one file to send (see --help)' });
    }
    const sent = await sendCommand({
      file: positionals[0],
      deviceId: values.device,
      ...deviceSeams(),
    });
    if (jsonMode) {
      // `destination` is OMITTED for reMarkable, which has no path; `uploaded`
      // takes its place. Two shapes, one object, per the design spec. Which
      // shape is READ OFF SendResult.uploaded, never re-derived here: two
      // places deciding the same thing is two places that can disagree.
      console.log(
        JSON.stringify({
          ok: true,
          device: sent.device,
          ...(sent.uploaded ? { uploaded: true } : { destination: sent.destination }),
        }),
      );
      return;
    }
    console.log(
      sent.uploaded
        ? `sent ${basename(positionals[0])} to ${sent.device.name}`
        : `sent ${basename(positionals[0])} to ${sent.device.name}: ${sent.destination}`,
    );
  } catch (err) {
    if (err instanceof CliError) fail(err.toJson());
    throw err;
  }
}

async function main() {
  // Dispatch BEFORE parseArgs: a verb's flags are not the conversion flags.
  // jsonMode's raw-argv pre-scan above already holds for this path, so a
  // throw inside the verb parser still exits as one JSON object.
  const command = resolveCommand(process.argv.slice(2));
  if (command.kind === 'verb') {
    await runVerb(command.verb, command.args);
    return;
  }

  let parsed: ReturnType<typeof parseCliArgs>;
  try {
    parsed = parseCliArgs();
  } catch (err) {
    fail({ code: 'usage', message: errorMessage(err) });
  }
  const { values, positionals } = parsed;
  jsonMode = values.json;

  if (values.version) {
    showVersion();
  }
  if (values.help) {
    showHelp();
  }
  if (positionals.length === 0) {
    if (jsonMode) {
      fail({ code: 'usage', message: 'expected exactly one input file (see --help)' });
    }
    console.log(USAGE);
    process.exit(1);
  }
  if (positionals.length > 1) {
    fail({ code: 'usage', message: 'expected exactly one input file' });
  }
  // Checked before any file I/O: without --json there is nowhere for the
  // document to go (stdout is human-readable text, not the app's decode
  // target), so failing here — rather than after a full conversion has
  // already written .epub/.fountain output — avoids doing (and writing)
  // work the caller cannot use.
  if (values['preview-inline'] && !jsonMode) {
    fail({
      code: 'usage',
      message: '--preview-inline needs --json: the document rides inside the result object',
    });
  }

  const input = positionals[0];
  const ext = extname(input).toLowerCase();
  // --fountain names where a PDF's .fountain is written. Beside
  // --no-fountain, one of the two would silently lose; on a .fountain or
  // .txt input, no .fountain is written at all (only a PDF produces one), so
  // the path it names would never appear. Both are refused before anything
  // is read, the same rule as --library with -o below. Any other extension
  // is left to the unsupported-type refusal further down, which says the
  // more useful thing.
  if (values.fountain !== undefined && values['no-fountain']) {
    fail({
      code: 'usage',
      message: 'pass --fountain or --no-fountain, not both (--no-fountain writes no .fountain for --fountain to name)',
    });
  }
  if (values.fountain !== undefined && (ext === '.fountain' || ext === '.txt')) {
    fail({
      code: 'usage',
      message: `a ${ext} input takes no --fountain (only a PDF conversion writes a .fountain)`,
    });
  }
  // -o already says where the output goes, so --library beside it says
  // nothing this run can act on. Rejected rather than ignored, the same way
  // --options with --options-json is: a flag that silently did nothing
  // teaches the caller it did something.
  if (values.library && values.output !== undefined) {
    fail({
      code: 'usage',
      message: 'pass --library or -o, not both: -o already says where the output goes',
    });
  }
  let format: Record<string, unknown> | undefined;
  if (values.options !== undefined && values['options-json'] !== undefined) {
    fail({
      code: 'bad-options',
      message: 'pass --options or --options-json, not both',
    });
  }
  if (values.options) {
    try {
      format = JSON.parse(await readFile(values.options, 'utf8'));
    } catch {
      // Its own code: "your options file is bad" and "the engine crashed"
      // demand different reactions from the caller, and the raw error
      // would leak a temp path into a user-facing message.
      fail({ code: 'bad-options', message: `cannot read options file ${values.options}` });
    }
  }
  if (values['options-json'] !== undefined) {
    // The app's channel: the window has no filesystem, so its settings
    // arrive as one argv element. The PAYLOAD never reaches the message —
    // it is a whole settings object and would bury the sentence.
    let parsed: unknown;
    try {
      parsed = JSON.parse(values['options-json']);
    } catch {
      fail({ code: 'bad-options', message: '--options-json is not valid JSON' });
    }
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      fail({ code: 'bad-options', message: '--options-json must be a JSON object' });
    }
    format = parsed as Record<string, unknown>;
  }

  // The script's own saved settings apply to THIS conversion — the one the
  // user just asked for — and not only to whatever happens to re-render the
  // book afterwards. They used to be read by `screepub settings` alone, so a
  // library conversion ADOPTED a sidecar and then rendered without it: the
  // first book off a tuned script came out at the defaults, and every rung
  // that converts the EPUB as it stands (export's KFX and AZW3, and every
  // non-Kindle device, which get the EPUB verbatim) shipped those defaults to
  // the reader.
  //
  // Scope: any conversion that finds this script's sidecar, not only
  // --library. A .screepub.json never appears by accident — `screepub
  // settings --set` is the only thing that writes one — so honouring it is
  // honouring something the user deliberately said about this script, and
  // the same file being obeyed or ignored depending on an unrelated output
  // flag would be the stranger rule. What keeps that from being a surprise
  // is that it is SAID, on stderr, every time it happens.
  //
  // Precedence: explicit flag > sidecar > app defaults > shipped defaults,
  // knob by knob, through the one merge resolveFormatOptions already is,
  // no second rule. A partial --options therefore moves the knobs it names
  // and leaves the rest of the script's tuning standing, which is what
  // cli-settings' --set does with the same call.
  //
  // Read once per conversion, not once per sidecar candidate below: the
  // user's own settings do not change mid-conversion, and two reads could
  // in principle disagree if the app settings file were rewritten between
  // them (the app changing it while a long conversion runs).
  const appDefaults = appDefaultOptions();
  // The Settings page's "When a PDF is converted" choice, read in the same
  // breath as the defaults it governs, so one conversion decides whether to
  // pin from the same look at the settings file it took its defaults from.
  // Only a --library conversion ever pins, so only one reads it.
  const keepSettings = values.library && keepsScriptSettings();
  let settings: FormatOptions | undefined;
  let settingsPath: string | undefined;
  const sidecarCandidates: string[] = [];
  // Resolved at most once here and reused below, rather than each of the
  // two --library uses calling libraryRoot() on its own: two separate
  // reads of the settings file could disagree with each other if the
  // chosen library folder changed between them, landing one conversion in
  // two different folders. Left undefined if resolving it here throws; the
  // second use recomputes it, and that is where the failure has always
  // been reported.
  let libRoot: string | undefined;
  if (values.library) {
    // A sidecar already IN the library outranks the older copy beside the
    // PDF — the same precedence adoptSidecar applies when it refuses to
    // overwrite it. Read-only: resolving it must not create a folder for an
    // input that is about to be refused.
    try {
      libRoot = libraryRoot();
      const prefix = existingLibraryOutput(input, libRoot);
      if (prefix !== null) sidecarCandidates.push(`${prefix}.fountain`);
    } catch {
      // An unusable library is the conversion's problem, and it is reported
      // below with its own code. It is not a reason to fail here.
    }
  }
  sidecarCandidates.push(input);
  for (const candidate of sidecarCandidates) {
    // The sidecar is read OVER the app defaults, not the shipped ones: a
    // knob the sidecar never mentions still comes from what the user chose
    // as their own starting point, not from Screepub's.
    const read = readScriptSettings(candidate, appDefaults);
    if (read === null) continue;
    if (read.settings === null) {
      // Malformed must never break a conversion that would otherwise
      // succeed — loadScriptSettings has always shrugged at one — but
      // shrugging SILENTLY is how "why does this look different from last
      // time" goes unanswered.
      process.stderr.write(
        `screepub: ignoring ${read.path}, which is not a settings object\n`,
      );
      continue;
    }
    settings = read.settings;
    settingsPath = read.path;
    process.stderr.write(
      `screepub: using this script's saved settings in ${read.path}` +
        `${format ? ' (the options you passed override them)' : ''}\n`,
    );
    break;
  }
  // One object, one merge: the flags over the sidecar over the app defaults
  // over the shipped ones. No sidecar applied, so the base one layer down is
  // the app defaults rather than convertPdf/convertFountain's own
  // shipped-defaults base. A FULL object here (resolveFormatOptions always
  // returns one) is what keeps that base from being resolved away when
  // convert.ts merges it again over DEFAULT_FORMAT_OPTIONS.
  const formatForConvert = settings === undefined
    ? resolveFormatOptions(format, appDefaults)
    : resolveFormatOptions(format, settings);

  // Progress goes to STDERR, never stdout. --json's contract is that stdout
  // is exactly one parseable object, and the app decodes it as such; a
  // progress line on stdout would corrupt every conversion the app runs.
  // Ticks are throttled to whole percents so a 300-page script emits ~100
  // lines rather than one per page per stage. A new stage is always news,
  // though: one can begin at the very percent the last one ended on, and
  // dropping that line would leave the window naming a stage that is over.
  let last = '';
  const onProgress = values.progress
    ? (stage: ConvertStage, fraction: number) => {
        const percent = Math.round(fraction * 100);
        const said = `${stage} ${percent}`;
        if (said === last) return;
        last = said;
        process.stderr.write(`${JSON.stringify({ progress: { stage, percent } })}\n`);
      }
    : undefined;

  const opts = { title: values.title, author: values.author, force: values.force, mobi: values.mobi, format: formatForConvert, onProgress };

  let result: ConvertResult;
  const isPdf = ext === '.pdf';
  try {
    if (isPdf) {
      result = await convertPdf(new Uint8Array(await readFile(input)), opts);
    } else if (ext === '.fountain' || ext === '.txt') {
      const text = await readFile(input, 'utf8');
      // The pictures this text's notes name sit in the folder a PDF
      // conversion wrote beside it, under the same stem, whichever of the two
      // extensions the text now carries. Read only, never written or pruned:
      // that folder belongs to the conversion that filled it, and this run
      // has no pictures of its own to put there.
      //
      // Not read at all with the option off. Nothing would be shown, and a
      // folder's warnings are about pictures missing from the book, which
      // someone who turned pictures off is not missing.
      let pictures: ImagesRead | undefined;
      if (formatForConvert.includeImages) pictures = await readImages(input);
      result = await convertFountain(text, { ...opts, images: pictures?.images });
      if (pictures) result.warnings.push(...imageFolderWarnings(pictures));
    } else {
      fail({
        code: 'unsupported-type',
        message:
          `unsupported input type "${ext}": expected .pdf, .fountain, or .txt` +
          verbHint(input),
      });
    }
  } catch (err) {
    if (err instanceof ScannedPdfError) {
      fail({ code: 'scanned', message: err.message });
    }
    if (err instanceof NotAScreenplayError) {
      fail({ code: 'not-screenplay', message: err.message });
    }
    const mapped = mapConversionError(err);
    if (mapped) fail(mapped);
    throw err;
  }

  // WHERE the output goes is decided only once there is output to put there.
  // Resolving the library earlier made a folder — and CLAIMED the plain stem
  // name, pushing the real script of that name into a hashed one — for every
  // typo'd path and every file that turned out not to be a screenplay.
  // Nothing above this line writes anything, so a refusal leaves the library
  // exactly as it found it.
  let inputStem = join(dirname(input), basename(input, extname(input)));
  if (values.library) {
    try {
      inputStem = libraryOutput(input, libRoot);
      // Tuning the user already did beside the PDF follows the script in,
      // so the library does not start it over at the defaults.
      adoptSidecar(input, inputStem);
    } catch (err) {
      // The app's contract holds even here: one JSON object, never a throw
      // from deep inside node:fs.
      //
      // This message DOES carry the raw node:fs text, path and all, where
      // bad-options deliberately does not. The difference: a bad --options
      // path is a temp file the app made and the user has never seen, while
      // this one is a folder in the user's own home that they are the only
      // person who can fix. "EACCES … mkdir '/home/ada/Documents/Screepub'"
      // is the whole of the fix; "cannot open the library folder" alone
      // would send them looking for a location we never named.
      fail({ code: 'library', message: `cannot open the library folder: ${errorMessage(err)}` });
    }
  }
  const epubPath = values.output ?? `${inputStem}.epub`;
  // Companion outputs (.mobi/.fountain/.elements.json) follow the EPUB, so
  // -o into a library folder keeps everything together.
  const stem = join(dirname(epubPath), basename(epubPath, extname(epubPath)));

  await writeFileAtomic(epubPath, result.epub);

  let mobiPath: string | undefined;
  if (values.mobi && result.mobi) {
    mobiPath = `${stem}.mobi`;
    await writeFileAtomic(mobiPath, result.mobi);
  }

  let fountainPath: string | undefined;
  let imagesPath: string | undefined;
  if (isPdf && !values['no-fountain']) {
    fountainPath = values.fountain ?? `${stem}.fountain`;
    // The picture folder goes with the .fountain, and only with it: beside
    // whichever .fountain this run writes (the script's library folder under
    // --library, beside the input, or wherever --fountain says), never beside
    // the EPUB. Its files exist to answer the `[[img: ...]]` notes in that
    // text, so suppress the text and the folder would be a pile of PNGs
    // nothing points at; --no-fountain writes neither.
    //
    // Pictures FIRST, so the .fountain never appears naming files that are
    // not there yet. Cheap ordering rather than a guarantee: nothing here is
    // atomic across files.
    const dir = imagesDirFor(fountainPath);
    if (result.images.length > 0) {
      try {
        await writeImages(dir, result.images);
      } catch (err) {
        const refused = unwritableError(
          dir, err, [epubPath, mobiPath].filter((p): p is string => p !== undefined),
        );
        if (refused) fail(refused);
        throw err;
      }
      imagesPath = dir;
    } else {
      // A draft that has lost its art still has to clear the last
      // conversion's, or the folder keeps pictures this .fountain no longer
      // names. Nothing is created here: a script with no pictures gets no
      // folder, and one emptied of ours is taken away with them. Unguarded
      // because the prune never throws (every step is best-effort).
      await pruneImages(dir, new Set());
    }
    await writeFileAtomic(fountainPath, result.fountainText, 'utf8');
  }
  let previewPath: string | undefined;
  if (values['preview-html']) {
    previewPath = values['preview-html'];
    await writeFileAtomic(previewPath, result.previewHtml, 'utf8');
  }
  let debugPath: string | undefined;
  if (values.debug && result.screenplay) {
    debugPath = `${stem}.elements.json`;
    await writeFile(debugPath, JSON.stringify(result.screenplay, null, 2), 'utf8');
  }

  // Placed after every write above, not right after the library folder
  // opened: the conversion can still fail past that point (the book's own
  // destination turning out to be a directory, a full disk), and a sidecar
  // left behind from a conversion whose book was never written would tell
  // `screepub settings` about a script that is not really there yet.
  //
  // A script that reaches the library with no sidecar of its own, a fresh
  // PDF, or an old one with nothing adopted above, has been reading its
  // settings off the app defaults on every conversion. Left that way, a
  // later "Reset new scripts to Screepub's defaults" or "Use these for new
  // scripts" reaches BACKWARD into a library already on a reader's device:
  // the book on disk was built at one set of knobs, and the next `screepub
  // settings` call for it silently starts answering with another. App
  // defaults are meant to be the starting point for NEW scripts, not a
  // live wire into every old one, so what this script started from becomes
  // its own the moment it first lands here.
  //
  // appDefaults, not formatForConvert: this only runs when there was no
  // sidecar to read, so formatForConvert here is exactly appDefaults with
  // any --options/--options-json baked on top, and a flag used for one
  // conversion must never freeze into the script's standing choice.
  //
  // PDF input only: a .fountain input never gets a .fountain written into
  // the library (only a PDF produces one, a few lines up), so a saved
  // sidecar here would have no library .fountain for `screepub settings`
  // to read it against, while still outranking the user's own sidecar
  // beside their .fountain input on the next conversion.
  //
  // And only while the app says to keep a script's settings (the default;
  // keepScriptSettings, spec 2026-09-24). Off, nothing is saved here and an
  // untuned script goes on following the app defaults. Off NEVER undoes a
  // save made while it was on: a sidecar this block wrote and one the
  // reader tuned are the same file, and nothing records which is which, so
  // removing or rewriting one here could wipe real tuning. The existsSync
  // check below already leaves any sidecar that is there alone; turning
  // the choice off adds nothing that touches one.
  if (keepSettings && isPdf) {
    // Checked fresh rather than trusted from adoptSidecar's return value:
    // the library can already hold a sidecar from an EARLIER conversion of
    // this same script (adoptSidecar only ever copies from beside the
    // PDF), and that earlier save, or a hand-edited one, must never be
    // overwritten.
    const pinPath = sidecarPath(inputStem);
    if (!existsSync(pinPath)) {
      try {
        saveScriptSettings(appDefaults, inputStem);
      } catch (err) {
        // The conversion already succeeded: the book is written and
        // correct. Losing this save loses only the CONVENIENCE of the
        // script staying put through a later app-default change, not the
        // book itself, so this is said and survived, the same shrug
        // loadScriptSettings already gives a sidecar it cannot read.
        process.stderr.write(
          `screepub: could not save this script's settings to ${pinPath}: ${errorMessage(err)}\n`,
        );
      }
    }
  }

  const sp = result.screenplay;
  if (jsonMode) {
    // sayLine, not console.log: this is the one answer that can outgrow a
    // pipe buffer, and the window on the other end of that pipe needs all
    // of it. See sayLine's note.
    await sayLine(
      JSON.stringify({
        ok: true,
        title: result.meta.title,
        author: result.meta.author,
        pages: sp?.pageCount,
        scenes: sp?.scenes.length,
        characters: sp?.characters.length,
        topCharacters: sp?.characters.slice(0, 5).map((c) => c.name) ?? [],
        warnings: result.warnings,
        // Word spaces pdf.js invented and we removed against the glyph
        // stream. Reported rather than hidden because it edits the author's
        // text; normally 0, and a number that jumps is how a misfire shows.
        spacingRepairs: result.spacingRepairs,
        epubPath,
        mobiPath,
        fountainPath,
        // Two different facts, and both are omitted when they are nothing
        // (JSON.stringify drops an undefined member, as it does for
        // mobiPath). `imagesPath` is where a picture folder was written, so it
        // is absent whenever none was. `imageCount` is how many pictures the
        // CONVERSION carried out of the PDF, a fact about the conversion and
        // not about anything on disk: under --no-fountain it is reported with
        // no folder beside it at all. Nor is it how many the book shows,
        // which depends on includeImages and on the format. A .fountain
        // input reports neither: it writes no folder and carries nothing out
        // of a PDF, even when its book shows the pictures it read.
        imagesPath,
        imageCount: result.images.length > 0 ? result.images.length : undefined,
        // The sidecar this conversion actually rendered with, so the window
        // (and anyone reading the answer later) can tell a book built from a
        // script's saved settings from one built at the defaults.
        settingsPath,
        previewHtmlPath: previewPath,
        debugPath,
        // Spread, not a plain key: the app asks for this and nothing else
        // does, and a megabyte of HTML on every conversion would be a tax
        // every other caller pays for one caller's convenience.
        ...(values['preview-inline'] ? { previewHtml: result.previewHtml } : {}),
      }),
    );
    return;
  }

  console.log(`${result.meta.title}${result.meta.author ? `, by ${result.meta.author}` : ''}`);
  if (sp) {
    const top = sp.characters
      .slice(0, 5)
      .map((c) => c.name)
      .join(', ');
    console.log(
      `  ${sp.pageCount} pages · ${sp.scenes.length} scenes · ` +
        `${sp.characters.length} speaking characters${top ? ` (${top}…)` : ''}`,
    );
  }
  for (const w of result.warnings) console.log(`  warning: ${w}`);
  for (const f of [epubPath, mobiPath, fountainPath, imagesPath, previewPath, debugPath]) {
    if (f) console.log(`  wrote ${f}`);
  }
}

// The last line of defense for the app contract: an error nobody
// anticipated (disk full, a pdf.js internal, an OOM-adjacent throw) must
// still come out as JSON on stdout, or the app surfaces a stack trace.
try {
  await main();
} catch (err) {
  if (jsonMode) {
    console.log(JSON.stringify({
      ok: false,
      error: { code: 'internal', message: String(err) },
    }));
  } else {
    console.error(err);
  }
  process.exit(1);
}
