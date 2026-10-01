// The picture folder beside a `.fountain`: `<stem>.images/`, written by a PDF
// conversion and read back by anything that renders a book from the text.
//
// ONE module for every caller, and that is the point of it. The CLI writes
// the folder when it writes the `.fountain`; whatever later builds a book from
// that `.fountain` (a re-render, the export path's Kindle fallback) has to
// find the same pictures under the same rules, and a second reader with its
// own idea of which files count is how a route ends up shipping a book with
// no art and no warning. Named for what it holds rather than for who calls it:
// "sidecar" in this repo already means a script's settings file and the
// engine binary.
//
// Separated from cli.ts for the reason cli-errors.ts states in its own first
// line: cli.ts runs `main()` on import, so nothing in it can be unit-tested,
// and the alternative is proving a filename predicate through a subprocess.
// Everything here is policy about someone else's directory (what may be read,
// what may be deleted, how much of it), which is exactly the kind of rule that
// deserves direct tests. convert.ts stays free of the filesystem: it hands the
// pictures out by NAME, and this is where they meet a disk.
//
// Writes go through atomic-write.ts, the engine's one temp-then-rename rule,
// and the temp name it uses (`tempPathFor`) is the shape `isImageTemp` below
// has to recognise when a crash leaves one behind.
import { basename, dirname, extname, join } from 'node:path';
import { lstat, mkdir, readdir, readFile, unlink, rmdir } from 'node:fs/promises';
import { writeFileAtomicSync } from './atomic-write';
// The one allowlist for a filename we are willing to carry (see notes.ts).
// A directory listing is no more trustworthy than a hand-edited .fountain.
import { isSafeImageFile } from './fountain/notes';
// A far NARROWER question, and the two must not be confused: what this
// engine's own image writer produces. Only these may be deleted.
import { isGeneratedImageFile, IMAGE_MAX_EDGE } from './parser/images';
// The name says `.png`; this asks the bytes. A file that fails it would be
// declared image/png in an OPF manifest and fail epubcheck.
import { isPng } from './parser/png';
// The PDF path's own ceiling on pictures per document, reused so a folder
// cannot bring in more than the conversion that filled it could produce.
import { MAX_IMAGES } from './parser/artwork';

/**
 * The picture folder that belongs to a `.fountain`: same directory, same stem.
 *
 * ONE derivation, shared by the write and the read. Deriving the write from
 * the EPUB's stem instead would look identical until someone passes
 * `--fountain elsewhere/x.fountain`, and then the notes would name pictures
 * that sit beside a book rather than beside the text that references them —
 * a re-render would find nothing, silently. In the library the `.fountain`
 * sits in the script's own folder, so this is that folder too: one rule, no
 * special case.
 */
export function imagesDirFor(fountainPath: string): string {
  return `${join(dirname(fountainPath), basename(fountainPath, extname(fountainPath)))}.images`;
}

/**
 * Biggest picture file we will read back: what this engine's own encoder can
 * emit at `IMAGE_MAX_EDGE`, plus slack.
 *
 * COMPUTED, not a hand-derived constant, because the input is tuned by
 * measurement elsewhere (see MAX_IMAGES in artwork.ts for a retune of exactly
 * that kind). Raising IMAGE_MAX_EDGE past ~1670 would have pushed our own
 * output over a literal 8 MiB and made the reader silently refuse the
 * conversion's own pictures — a bug whose symptom is a re-render losing art
 * for no visible reason.
 *
 * The arithmetic: a PNG at the cap is (edge*3+1)*edge bytes of scanlines
 * before deflate, which on incompressible pixels grows it by well under a
 * percent (stored blocks cost five bytes per 65535). Five percent is slack for
 * that plus chunk headers, and a test encodes a full-size picture of noise
 * against this number rather than trusting the sum.
 *
 * What it refuses is the 4 GB file named `x.png` that someone drops in a
 * folder we then `readFile` into memory on an ordinary conversion. With the
 * count cap that is 60 x 6 MB in the worst case, not 6 MB — the pair bounds a
 * folder to roughly what the PDF path that filled it held in memory anyway,
 * which is the point, rather than to anything small. The largest folder in the
 * local corpus is 16 MB for sixty pictures.
 */
export const MAX_IMAGE_FILE_BYTES = Math.ceil((IMAGE_MAX_EDGE * 3 + 1) * IMAGE_MAX_EDGE * 1.05);

/**
 * A leftover from an interrupted write in a picture folder: `.p001-01.png.99.tmp`.
 *
 * The dot-prefix, the pid and the suffix are `tempPathFor`'s shape
 * (atomic-write.ts); the NAME inside is `imageFileName`'s, and is checked with
 * the same predicate the prune uses rather than spelled out a third time.
 * A write that FAILS takes its temp with it (atomic-write.ts removes it before
 * rethrowing), so the only way one survives is a process killed between the
 * write and the rename. It is invisible to `readImages` (it is not a `.png`),
 * and nothing else would ever remove it.
 */
const TEMP_SHAPE = /^\.(.+)\.\d+\.tmp$/;

export function isImageTemp(name: string): boolean {
  const m = TEMP_SHAPE.exec(name);
  return m !== null && isGeneratedImageFile(m[1]);
}

/** Everything a caller needs from a picture folder: the pictures, and how many
 * were there and could not be had. */
export interface ImagesRead {
  images: Map<string, Uint8Array>;
  /**
   * Entries that named a picture and did not yield one: not a regular file,
   * larger than anything we could have written, not a PNG once opened, or an
   * error on the read.
   *
   * NOT a count of everything skipped. A `notes.txt` in the folder is not a
   * lost picture, and neither is a name no note could carry; those are refused
   * before they are ever candidates. Nor is a picture the CAP turned away —
   * that one is `overCap`, below, because it could be read perfectly well.
   * This is the number that separates "this script has no art" from "this
   * script's art is on a drive that is not here", which on the Fountain path
   * is the only signal there is.
   *
   * Only the script's own names count when the caller says what those are
   * (`readImages`'s `asked`); the same goes for `overCap`.
   */
  refused: number;
  /**
   * Names the CAP turned away: pictures the folder held that this conversion
   * had no room for.
   *
   * The Fountain path's answer to `ArtworkVerdict.overCap`, and its own count
   * rather than part of `refused` for the reason `imageWarnings` keeps the two
   * apart on the PDF side: they are different sentences. A refused picture
   * could not be read; these could, and were left out because a book has a
   * ceiling. Rolling them together would tell someone their drive was missing
   * when their folder was merely full.
   *
   * Counts NAMES, not verified pictures, and that is the same deliberate trade
   * `ImageStats.failed` makes one path over. Past the cap nothing is opened,
   * so a directory called `p061-01.png` is counted here as if it were a plate.
   * The alternative is reading files to decide whether to report not reading
   * them. What bounds the overcount is the allowlist the listing was already
   * filtered through: only names a note could carry ever reach this loop.
   */
  overCap: number;
  /**
   * Pictures the caller asked for that the folder does not list at all: a
   * file deleted, an iCloud copy evicted (which renames it to
   * `.p002-01.png.icloud`, a name no note can carry), or no folder there.
   *
   * The one loss the counts above cannot see, since each of them is about a
   * name the folder DID list. Kept apart from them so that every picture the
   * script shows and the book lacks is counted exactly once, under the most
   * specific cause the read knows: a listed name it would not read is
   * `refused`, a listed name past the cap is `overCap`, a folder that is a
   * link or a file is `folderRefused` (and this is 0, since nothing in it was
   * looked at), and only a name the listing never offered lands here.
   *
   * Zero unless the caller says what it is looking for (`readImages`'s
   * `asked`): a folder alone cannot know which of its pictures are missing.
   */
  notFound: number;
  /**
   * Present, and true, when `<stem>.images` exists but is not a folder of its
   * own: a symlink (wherever it points) or a file. Nothing in it is read.
   *
   * OPTIONAL and absent on the happy path, like `ImageStats.decodersUnavailable`,
   * because it is not a count: it says the whole folder was refused, which is
   * the one reason for an empty read a user can act on.
   */
  folderRefused?: true;
}

/**
 * What is at a picture folder's path: nothing, a real folder, or something
 * this module will not touch (a symlink, wherever it points, or a file).
 *
 * `lstat`, for the reason the entry gate in `readImages` uses it: everything
 * that opens a path follows a link, so a `<stem>.images` that is a symlink
 * would be read from, written into and pruned as if it were ours, and the
 * prune deletes this engine's own picture names in whatever folder the link
 * names. Nothing this engine writes is ever a symlink, so refusing one costs
 * nothing real.
 *
 * Any failure to ask counts as `missing`, which is what the callers did with
 * a folder they could not list before this check existed: an empty read, and
 * nothing to prune.
 */
async function folderAt(dir: string): Promise<'missing' | 'folder' | 'refused'> {
  try {
    return (await lstat(dir)).isDirectory() ? 'folder' : 'refused';
  } catch {
    return 'missing';
  }
}

/**
 * Image bytes beside a `.fountain`, for rendering a book from it.
 *
 * Missing folder, missing file, unreadable file and a file too big to be one
 * of ours all mean the same thing: render the text without that picture. That
 * is what someone gets when they have moved the `.fountain` and left the
 * folder behind, and a hard failure there would turn a cosmetic loss into a
 * dead conversion. Each file is tried on its own, so one bad name or one
 * locked file costs its own picture and not the ones listed after it. The
 * losses are COUNTED rather than swallowed — see `ImagesRead.refused`.
 *
 * Bounded on six axes, because none of this comes from us:
 *
 *   - the FOLDER has to be a folder of its own (see `folderAt`). A symlinked
 *     or file-shaped one is refused whole, and said so (`folderRefused`);
 *   - the NAME has to be one a note could have written, since it goes on to be
 *     a zip entry path and an href in an EPUB manifest;
 *   - the entry has to be a REGULAR FILE, and `lstat` is what asks — `stat`
 *     answers for the target, so a symlink named `p004-01.png` pointing at any
 *     file the user can read is carried into the book as a picture. Measured:
 *     it comes back holding /etc/hosts. That is a confused deputy the moment a
 *     book is packaged and mailed. The same test refuses a FIFO, which blocks
 *     `readFile` at OPEN forever, and a `/dev/zero` symlink, which opens fine
 *     and then throws ENOMEM after reading for about a second. A symlinked
 *     picture is refused with them, deliberately: nothing this engine writes
 *     is ever a symlink.
 *   - the file has to be no LARGER than our own encoder could have made it;
 *   - the CONTENT has to be the picture the name claims. The manifest declares
 *     `image/png` on the strength of the extension and epubcheck opens the
 *     file, so a JPEG someone renamed, or a 0-byte iCloud placeholder, is a
 *     book that fails validation. This axis is counted like the others rather
 *     than filtered quietly downstream: it is exactly the case the warning
 *     below exists for — art that is on a drive that is not here, or is no
 *     longer art — and the alternative was one plate silently missing from a
 *     conversion that reported no warnings at all;
 *   - and no more may be ACCEPTED than a PDF conversion could have carried.
 *     The cap is on what survives the gates above, not on what the directory
 *     lists — capping the listing let sixty directories named `p001-01.png`
 *     evict the one real picture behind them, which is the same eviction the
 *     name check closes for junk names, through a door the name check does not
 *     reach.
 *
 * Names are taken in order, so a folder holding more than the cap yields the
 * same 60 pictures on every machine.
 *
 * `asked` is the set of pictures the text's notes name (`imageNoteFiles`),
 * and it changes nothing about what is read. It lets the read count the ones
 * the folder does not list (`ImagesRead.notFound`), and, when it names any,
 * narrows `refused` and `overCap` to its own names: a stray file no note names
 * is no loss to the book, and the window's Tune would otherwise say a picture
 * was left out of one that lacks none. Empty, as the Kindle rebuild passes it,
 * every name counts. A folder that is missing, or that vanished between the
 * check and the listing, lists none.
 *
 * Asynchronous where the writer below is not, and for the FIFO's sake: should
 * the `lstat` gate ever regress, a FIFO blocks `readFile` at open, and an
 * asynchronous open blocks one worker thread where a synchronous one would
 * freeze the whole process, test runner included, past any timeout that could
 * report it.
 */
export async function readImages(
  fountainPath: string,
  asked: ReadonlySet<string> = new Set(),
): Promise<ImagesRead> {
  const dir = imagesDirFor(fountainPath);
  const images = new Map<string, Uint8Array>();
  let refused = 0;
  let overCap = 0;
  const at = await folderAt(dir);
  if (at === 'missing') return { images, refused, overCap, notFound: asked.size };
  if (at === 'refused') return { images, refused, overCap, notFound: 0, folderRefused: true };
  let names: string[];
  try {
    names = (await readdir(dir)).filter(isSafeImageFile).sort();
  } catch {
    // Gone or unlistable since the check.
    return { images, refused, overCap, notFound: asked.size };
  }
  const listed = new Set(names);
  const notFound = [...asked].filter((name) => !listed.has(name)).length;
  // Told what the script shows, a loss is only a loss if it is one of those:
  // a stray file no note names costs the book nothing, and counting it would
  // have Tune say a picture was left out of a book that lacks none. Told
  // nothing, every name a note could carry might be one, as before.
  const counts = (name: string) => asked.size === 0 || asked.has(name);
  for (const name of names) {
    // `continue` and a count, not `break`. A `break` would make the cap the
    // one loss in this whole feature that nothing anywhere reports: the PDF
    // path warns about exactly this (`overCap`), and a folder read back would
    // lose plates in silence. Nothing below the cap runs for these, so the
    // cost of walking the tail is one loop iteration each.
    if (images.size >= MAX_IMAGES) {
      if (counts(name)) overCap++;
      continue;
    }
    const path = join(dir, name);
    try {
      const info = await lstat(path);
      if (!info.isFile() || info.size > MAX_IMAGE_FILE_BYTES) {
        if (counts(name)) refused++;
        continue;
      }
      // The content gate has to come after the read, which is the one thing
      // that separates it from its neighbours above: a header cannot be
      // stat'ed. The cost is bounded by the size gate that just ran.
      const bytes = new Uint8Array(await readFile(path));
      if (!isPng(bytes)) {
        if (counts(name)) refused++;
        continue;
      }
      images.set(name, bytes);
    } catch {
      if (counts(name)) refused++; // one picture lost, the rest unaffected
    }
  }
  return { images, refused, overCap, notFound };
}

/**
 * What to say about pictures a folder held and the book does not show.
 *
 * The Fountain path's answer to `imageWarnings`, and it mirrors that function
 * on purpose, down to which losses are worth a sentence: the two EXCEPTIONAL
 * ones, and neither of the ordinary ones. A book that ships with no art and no
 * error reads exactly like a script that never had pictures, and someone who
 * converts a PDF and then re-renders the `.fountain` must not be told two
 * different stories about one ceiling.
 *
 *   - `overCap`: the folder holds more than a book may carry. Reachable here
 *     in a way it is not on the PDF side, since that path writes at most
 *     MAX_IMAGES and this one reads a directory anybody may add to: one stray
 *     `art.png` sorts ahead of every `pNNN-NN.png` and takes the last plate's
 *     place.
 *   - `refused`: pictures the folder would not give up. A hand-assembled
 *     folder could speak for itself, but the caller that matters is the app
 *     re-rendering from its own library, where nobody assembled anything and
 *     the cause is an iCloud placeholder, a moved library or a permissions
 *     change.
 *   - `folderRefused`: the folder was a link or a file, so nothing in it was
 *     read at all. Said because the other two cannot say it: both are zero.
 *   - `notFound`: pictures the script shows that the folder does not hold
 *     under any name it lists. The window's Tune re-renders the library book
 *     from the `.fountain` on every knob move, so this is a figure gone from
 *     the book on disk, and without the sentence that re-render reports no
 *     warnings at all. Its own sentence because it is a different place to
 *     look: a file that is not there, rather than one that is there and would
 *     not be read.
 *
 * Takes the whole `ImagesRead` rather than its numbers, for `imageWarnings`'s
 * reason: `kept`, `refused` and `overCap` are three bare numbers with nothing
 * to stop them arriving in the wrong order, and two of them appear in one
 * sentence.
 */
export function imageFolderWarnings(read: ImagesRead): string[] {
  const out: string[] = [];
  if (read.folderRefused) {
    out.push(
      "This script's picture folder is not a folder of its own (it is a link or a file), so no pictures were read from it.",
    );
  }
  const kept = read.images.size;
  if (read.overCap > 0) {
    out.push(
      `Screepub found ${kept + read.overCap} pictures in this script's picture folder. ` +
        `Only the first ${kept} were used.`,
    );
  }
  if (read.refused > 0) {
    out.push(
      read.refused === 1
        ? "1 picture could not be read out of this script's picture folder, so it was left out."
        : `${read.refused} pictures could not be read out of this script's picture folder, so they were left out.`,
    );
  }
  if (read.notFound > 0) {
    out.push(
      read.notFound === 1
        ? '1 picture this script shows could not be found in its picture folder, so the book was made without it.'
        : `${read.notFound} pictures this script shows could not be found in its picture folder, so the book was made without them.`,
    );
  }
  return out;
}

/**
 * Remove this writer's OWN stale files from a picture folder, and say how many.
 *
 * `keep` is what the current run just wrote. Everything else that matches
 * `isGeneratedImageFile` or `isImageTemp` goes; nothing else is touched.
 *
 * NOT `isSafeImageFile`: that one is the read side's allowlist, deliberately
 * wide enough to accept any name a hand-written note could carry. Pruning
 * against it would delete a user's own `board-01.png` and `sunset.png` from a
 * directory they filled themselves (measured: it deletes both). So the prune
 * is bounded to names our own writer could have produced, and
 * `isGeneratedImageFile` is that set.
 *
 * Never creates the folder: a conversion that carries no pictures still has to
 * clear the last one's, and it must not leave an empty folder behind for a
 * script that has no art.
 *
 * Never reaches through a link either. The CLI calls this on its own when a
 * conversion has no pictures, so the folder check (`folderAt`) is made here
 * and not only in `writeImages`: a symlinked folder is left alone and nothing
 * is removed, wherever it points.
 */
export async function pruneImages(dir: string, keep: Set<string>): Promise<number> {
  if ((await folderAt(dir)) !== 'folder') return 0;
  let existing: string[];
  try {
    existing = await readdir(dir);
  } catch {
    return 0; // no folder to clean
  }
  let removed = 0;
  for (const name of existing) {
    if (keep.has(name)) continue;
    if (!isGeneratedImageFile(name) && !isImageTemp(name)) continue;
    // A stale picture we cannot delete is not worth failing a conversion that
    // otherwise produced everything it promised.
    await unlink(join(dir, name))
      .then(() => {
        removed++;
      })
      .catch(() => {});
  }
  // Only after taking something out, and only if that emptied it: a folder we
  // did not touch is not ours to remove, and `rmdir` refuses a folder with
  // anything left in it, so this can never reach a file somebody else put
  // there.
  if (removed > 0) await rmdir(dir).catch(() => {});
  return removed;
}

/**
 * Write the picture folder, and clear what the last conversion left in it.
 *
 * The prune is not tidiness. Filenames are per page, so re-converting a script
 * whose art has moved leaves the old numbering behind, and whatever packages a
 * book packages what the folder holds — a panel from two drafts ago would ship
 * inside it.
 *
 * An existing folder is adopted rather than refused. Refusing would leave a
 * `.fountain` full of notes with no pictures to answer them, which is a worse
 * failure than a folder that gains files; and since nothing but our own output
 * is ever deleted, adopting costs the user nothing they had.
 *
 * Only a FOLDER, though. A symlink at this path, wherever it points, or a
 * file, is refused before anything is written, with ENOTDIR (which the CLI
 * reports as `unwritable`). Adopting a link writes the pictures into its
 * target and then prunes the target, deleting any of this engine's picture
 * names it holds, which is the one data loss the prune's narrow name test
 * cannot prevent on its own.
 *
 * Each file lands atomically (atomic-write.ts: temp then rename), so a
 * conversion racing a reader is never observed half-written. The FOLDER is
 * not atomic: a reader can catch it with some pictures present and others
 * still to come, which comes out as a render missing a picture rather than as
 * a corrupt one. Two conversions racing INTO one folder is the one case the
 * temp sweep can bite: the loser's in-flight temp can be swept and its rename
 * fails. That is already a folder two processes are fighting over the same
 * output paths in, and failing loudly is the better half of it.
 *
 * Throws what the filesystem threw, untranslated: which error code that
 * becomes, and what the caller says about the files already written, is the
 * caller's business (the CLI's `unwritable`).
 */
export async function writeImages(
  dir: string,
  images: readonly { name: string; bytes: Uint8Array }[],
): Promise<void> {
  // The folder and never its parents, so not `recursive`. The parent is
  // wherever the .fountain goes, and when that is not there the path is
  // wrong: building it would put the pictures somewhere nobody asked for, so
  // a missing parent throws ENOENT and the caller reports it. EEXIST is
  // whatever already holds the name, which the check after it sorts into the
  // folder to adopt and the link or file to refuse.
  await mkdir(dir).catch((err: NodeJS.ErrnoException) => {
    if (err.code !== 'EEXIST') throw err;
  });
  if ((await folderAt(dir)) !== 'folder') {
    throw Object.assign(new Error(`${dir} is not a folder of its own (a link or a file)`), {
      code: 'ENOTDIR',
    });
  }
  const written = new Set<string>();
  for (const img of images) {
    writeFileAtomicSync(join(dir, img.name), img.bytes);
    written.add(img.name);
  }
  await pruneImages(dir, written);
}
