// The picture folder beside a `.fountain`, tested directly.
//
// Everything here would be a subprocess round-trip if it lived in cli.ts,
// which runs main() on import — so a filename predicate could only be observed
// through a whole conversion, and two constants would be pinned by nothing at
// all. cli.test.ts keeps the cases that are about the CLI's WIRING (which
// folder, beside which file, reported how); these are about the policy.
import { afterAll, describe, expect, test } from 'bun:test';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { readdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import {
  imageFolderWarnings,
  imagesDirFor,
  isImageTemp,
  MAX_IMAGE_FILE_BYTES,
  pruneImages,
  readImages,
  writeImages,
} from '../src/library-images';
import { tempPathFor, writeFileAtomicSync } from '../src/atomic-write';
import { IMAGE_MAX_EDGE } from '../src/parser/images';
import { encodePng } from '../src/parser/png';
import { MAX_IMAGES } from '../src/parser/artwork';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-library-images-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

let seq = 0;
/** A `.fountain` path with a folder of the given files beside it. */
function cached(files: Record<string, Uint8Array> | null): string {
  const dir = join(SCRATCH, `case-${seq++}`);
  mkdirSync(dir);
  const fountain = join(dir, 'a.fountain');
  writeFileSync(fountain, 'Title: T\n');
  if (files) {
    mkdirSync(join(dir, 'a.images'));
    for (const [name, bytes] of Object.entries(files)) {
      writeFileSync(join(dir, 'a.images', name), bytes);
    }
  }
  return fountain;
}

/**
 * A REAL one-pixel PNG, told apart from another by the colour of that pixel.
 *
 * Real because the reader checks content (`isPng`): filler bytes under this
 * name would be refused by the content gate, and a test about some other gate
 * would pass for that reason while claiming to prove something else. So the
 * helper produces what it is named after, and identity rides on the bytes
 * rather than the length.
 */
const png = (tag = 0) => encodePng(new Uint8Array([tag, 0, 0]), 1, 1);

/**
 * A folder somewhere else holding one of this engine's own picture names, and
 * a `.fountain` whose `<stem>.images` is a symlink to it. The name inside is
 * one the prune would delete and the reader would accept, so following the
 * link is visible from every side: a read finds it, a prune removes it, a
 * write lands beside it.
 */
function linkedFolder(): { fountain: string; dir: string; elsewhere: string } {
  const elsewhere = join(SCRATCH, `elsewhere-${seq++}`);
  mkdirSync(elsewhere);
  writeFileSync(join(elsewhere, 'p009-01.png'), png(9));
  const fountain = cached(null);
  const dir = imagesDirFor(fountain);
  symlinkSync(elsewhere, dir);
  return { fountain, dir, elsewhere };
}

/** A PNG header followed by padding, for the tests that are about SIZE. */
function pngOfSize(bytes: number): Uint8Array {
  const out = new Uint8Array(bytes);
  out.set(png().subarray(0, Math.min(bytes, png().length)));
  return out;
}

describe('imagesDirFor', () => {
  test('is the .fountain\'s own directory and stem', () => {
    expect(imagesDirFor('/lib/Book.fountain')).toBe('/lib/Book.images');
    expect(imagesDirFor('/lib/sub/A Long Title.fountain')).toBe('/lib/sub/A Long Title.images');
  });

  test('does not care what the extension was, only that it goes', () => {
    expect(imagesDirFor('/lib/Book.txt')).toBe('/lib/Book.images');
    expect(imagesDirFor('/lib/Book')).toBe('/lib/Book.images');
  });

  test('a dotted name keeps everything but the last extension', () => {
    expect(imagesDirFor('/lib/Draft.v2.fountain')).toBe('/lib/Draft.v2.images');
  });
});

describe('MAX_IMAGE_FILE_BYTES', () => {
  // The number is computed from IMAGE_MAX_EDGE rather than written down, and
  // this is the measurement behind the arithmetic: the biggest PNG this
  // engine can emit is a full-size picture of incompressible noise, and if it
  // ever exceeded this bound the reader would refuse the conversion's own
  // output — a re-render silently losing art for no visible reason.
  test('the largest picture this engine can encode fits inside it', () => {
    const px = IMAGE_MAX_EDGE * IMAGE_MAX_EDGE * 3;
    const noise = new Uint8Array(px);
    // Deterministic, and deliberately incompressible: an LCG, not Math.random,
    // so a failure here is reproducible.
    let s = 12345;
    for (let i = 0; i < px; i++) {
      s = (Math.imul(s, 1103515245) + 12345) >>> 0;
      noise[i] = (s >>> 16) & 0xff;
    }
    const bytes = encodePng(noise, IMAGE_MAX_EDGE, IMAGE_MAX_EDGE);
    expect(bytes.length).toBeGreaterThan(px); // really is incompressible
    expect(bytes.length).toBeLessThanOrEqual(MAX_IMAGE_FILE_BYTES);
  }, 60000);

  test('and it is not so loose that a 4 GB file would pass', () => {
    expect(MAX_IMAGE_FILE_BYTES).toBeLessThan(16 * 1024 * 1024);
  });
});

describe('isImageTemp', () => {
  test('recognizes the temp name the writer actually uses', () => {
    // Asked of atomic-write.ts, not reconstructed. Spelling the temp name out
    // here would be a second copy of the writer's format, which is the drift
    // this predicate exists to survive: `tempPathFor` is where every write in
    // writeImages gets its temp path, so its answer is the shape to match.
    const name = basename(tempPathFor(join(SCRATCH, 'p001-01.png')));
    expect(isImageTemp(name)).toBe(true);
  });

  test('a write that fails leaves no temp behind, so only a crash can', async () => {
    // Why the prune's temp half is about crashes alone: atomic-write.ts
    // removes its temp before rethrowing. Make the rename fail (the target is
    // a directory) and nothing but the directory is left.
    const dir = join(SCRATCH, 'temp-fail');
    mkdirSync(dir);
    mkdirSync(join(dir, 'p001-01.png'));
    expect(() => writeFileAtomicSync(join(dir, 'p001-01.png'), png())).toThrow();
    expect(await readdir(dir)).toEqual(['p001-01.png']);
  });

  test('a crashed write\'s temp file is swept, and only that', async () => {
    // A process killed between the write and the rename leaves its temp at
    // exactly `tempPathFor(target)`, invisible to `readImages` (not a `.png`)
    // and removed by nothing else. Planted at that path, the way the crash
    // would leave it, beside a real picture this run kept.
    const dir = join(SCRATCH, 'temp-crash');
    mkdirSync(dir);
    writeFileAtomicSync(join(dir, 'p001-01.png'), png());
    writeFileSync(tempPathFor(join(dir, 'p002-01.png')), png().subarray(0, 10));
    expect((await readdir(dir)).length).toBe(2);
    expect(await pruneImages(dir, new Set(['p001-01.png']))).toBe(1);
    expect(await readdir(dir)).toEqual(['p001-01.png']);
  });

  test('a temp file for something that is not one of our pictures is not ours', () => {
    expect(isImageTemp('.board-01.png.42.tmp')).toBe(false);
    expect(isImageTemp('.out.epub.42.tmp')).toBe(false);
    expect(isImageTemp('.notes.md.42.tmp')).toBe(false);
  });

  test('a plain file is never a temp, however it is named', () => {
    for (const name of ['p001-01.png', 'p001-01.png.tmp', '.p001-01.png.tmp', 'x.tmp']) {
      expect(isImageTemp(name)).toBe(false);
    }
  });
});

describe('readImages', () => {
  test('a missing folder is empty and blameless', async () => {
    expect(await readImages(cached(null)))
      .toEqual({ images: new Map(), refused: 0, overCap: 0, notFound: 0 });
  });

  test('an empty folder likewise', async () => {
    expect(await readImages(cached({})))
      .toEqual({ images: new Map(), refused: 0, overCap: 0, notFound: 0 });
  });

  test('reads the pictures, keyed by name', async () => {
    const { images, refused } = await readImages(
      cached({ 'p001-01.png': png(4), 'p002-01.png': png(9) }),
    );
    expect([...images.keys()]).toEqual(['p001-01.png', 'p002-01.png']);
    // The bytes of the right file under the right key: two pictures the same
    // length would pass a length check whichever way they were paired.
    expect(images.get('p002-01.png')).toEqual(png(9));
    expect(refused).toBe(0);
  });

  test('only names a note could have written, and they are not refusals', async () => {
    // A stray notes.txt is not a lost picture, so it must not warn: these are
    // refused before they are ever candidates.
    const { images, refused } = await readImages(
      cached({
        'p001-01.png': png(),
        'a&b.png': png(),
        'sneaky.png.txt': png(),
        'notes.txt': png(),
        'UPPER.PNG': png(),
      }),
    );
    expect([...images.keys()]).toEqual(['p001-01.png']);
    expect(refused).toBe(0);
  });

  test('a file bigger than anything we could have written is refused and counted', async () => {
    // A real header on it, so the refusal is the SIZE gate and not the
    // content one standing in for it.
    const { images, refused } = await readImages(
      cached({ 'p001-01.png': png(), 'p002-01.png': pngOfSize(MAX_IMAGE_FILE_BYTES + 1) }),
    );
    expect([...images.keys()]).toEqual(['p001-01.png']);
    expect(refused).toBe(1);
  });

  test('a file exactly at the bound is still ours', async () => {
    const { images, refused } = await readImages(
      cached({ 'p001-01.png': pngOfSize(MAX_IMAGE_FILE_BYTES) }),
    );
    expect(images.size).toBe(1);
    expect(refused).toBe(0);
  });

  test('a picture that is not one is refused, and COUNTED', async () => {
    // The gate this list gained last, and the reason it is here rather than
    // downstream: `refused` is what becomes a warning. A JPEG somebody
    // renamed is ERROR(OPF-029) in the book we would otherwise ship, and a
    // 0-byte file is the iCloud placeholder case — the realistic one.
    const found = await readImages(
      cached({
        'p001-01.png': png(),
        'p002-01.png': new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46]),
        'p003-01.png': new Uint8Array(0),
        'p004-01.png': png().subarray(0, 28),
      }),
    );
    expect([...found.images.keys()]).toEqual(['p001-01.png']);
    expect(found.refused).toBe(3);
    // and it is loud rather than quiet: this is what the CLI prints.
    expect(imageFolderWarnings(found)[0]).toContain('3 pictures could not be read');
  });

  test('a symlink is not followed, whatever it points at', async () => {
    // The target is a REAL PNG outside the folder, so the entry check is the
    // only gate that can refuse it: `stat` answers for the target, which
    // passes the size and content gates and would carry a file from anywhere
    // on the disk into the book. `lstat` asks about the entry, and an entry
    // that is a symlink is not a regular file.
    const fountain = cached({ 'p001-01.png': png() });
    const outside = join(SCRATCH, 'outside.png');
    writeFileSync(outside, png(7));
    symlinkSync(outside, join(imagesDirFor(fountain), 'p004-01.png'));
    const { images, refused } = await readImages(fountain);
    expect([...images.keys()]).toEqual(['p001-01.png']);
    expect(refused).toBe(1);
  });

  test('a picture folder that is itself a symlink is not followed, and says so', async () => {
    // The gate above asks about each ENTRY; this is the folder. Followed, a
    // link named `a.images` carries whatever folder it points at into the
    // book, which is the same confused deputy one level up.
    const { fountain } = linkedFolder();
    const read = await readImages(fountain);
    expect(read.images.size).toBe(0);
    expect(read.folderRefused).toBe(true);
    const warnings = imageFolderWarnings(read);
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain('picture folder');
    expect(warnings[0]).toContain('not a folder');
  });

  test('so is a file where the folder should be', async () => {
    const fountain = cached(null);
    writeFileSync(imagesDirFor(fountain), png());
    const read = await readImages(fountain);
    expect(read.images.size).toBe(0);
    expect(read.folderRefused).toBe(true);
    expect(imageFolderWarnings(read)).toHaveLength(1);
  });

  test('a directory named like a picture is refused, not read', async () => {
    const fountain = cached({ 'p001-01.png': png() });
    mkdirSync(join(imagesDirFor(fountain), 'p002-01.png'));
    const { images, refused } = await readImages(fountain);
    expect([...images.keys()]).toEqual(['p001-01.png']);
    expect(refused).toBe(1);
  });

  test('a pipe is refused without ever being opened', async () => {
    // A FIFO with no writer blocks at OPEN, so a reader that got as far as
    // readFile would hang the conversion rather than fail it. If this ever
    // regresses it fails by TIMEOUT, which is the shape of the bug.
    const fountain = cached({ 'p001-01.png': png() });
    const made = Bun.spawnSync(['mkfifo', join(imagesDirFor(fountain), 'p002-01.png')]);
    expect(made.exitCode).toBe(0);
    const { images, refused } = await readImages(fountain);
    expect([...images.keys()]).toEqual(['p001-01.png']);
    expect(refused).toBe(1);
  }, 30000);

  test('an unreadable file costs its own picture and no other', async () => {
    const fountain = cached({ 'p001-01.png': png(), 'p002-01.png': png() });
    const locked = join(imagesDirFor(fountain), 'p001-01.png');
    chmodSync(locked, 0o000);
    const { images, refused } = await readImages(fountain);
    chmodSync(locked, 0o644);
    // The one listed AFTER the failure still arrives.
    expect([...images.keys()]).toEqual(['p002-01.png']);
    expect(refused).toBe(1);
  });

  test('no more are accepted than a PDF conversion could have carried', async () => {
    const many: Record<string, Uint8Array> = {};
    for (let i = 1; i <= MAX_IMAGES + 5; i++) {
      many[`p${String(i).padStart(3, '0')}-01.png`] = png();
    }
    const { images } = await readImages(cached(many));
    expect(images.size).toBe(MAX_IMAGES);
    // In NAME order, so the same 60 on every machine.
    expect([...images.keys()][0]).toBe('p001-01.png');
    expect([...images.keys()].at(-1)).toBe(`p0${MAX_IMAGES}-01.png`);
  });

  test('the ones the cap turned away are COUNTED, not dropped in silence', async () => {
    // The PDF path treats this exact loss as worth a warning (`overCap` in
    // convert.ts), on the grounds that silent truncation reads as "we carried
    // everything". The folder path counts it the same way: readImages walks
    // past the cap with `continue` and counts each name it turns away, so
    // five perfectly good plates over the ceiling are reported, not lost with
    // `refused` at 0 and nothing said anywhere.
    //
    // Its OWN count rather than `refused`, because the two are different
    // sentences. A refused picture could not be read; these could, and were
    // not carried because the book has a ceiling.
    const many: Record<string, Uint8Array> = {};
    for (let i = 1; i <= MAX_IMAGES + 5; i++) {
      many[`p${String(i).padStart(3, '0')}-01.png`] = png();
    }
    const { images, refused, overCap } = await readImages(cached(many));
    expect(images.size).toBe(MAX_IMAGES);
    expect(refused).toBe(0);
    expect(overCap).toBe(5);
  });

  test('a name no note could carry is not an over-cap loss either', async () => {
    // The cap counts what it turned away, and junk was never on its way in.
    // Without this the fix would report a folder of rubbish as a book missing
    // its art.
    const files: Record<string, Uint8Array> = {};
    for (let i = 0; i < MAX_IMAGES + 5; i++) files[`a&b${String(i).padStart(3, '0')}.png`] = png();
    files['p001-01.png'] = png();
    const { images, refused, overCap } = await readImages(cached(files));
    expect([...images.keys()]).toEqual(['p001-01.png']);
    expect(refused).toBe(0);
    expect(overCap).toBe(0);
  });

  test('junk cannot occupy a place in that cap', async () => {
    // Two doors into the same eviction. Junk NAMES sort ahead of `p...` and
    // would fill the cap if the name check were only an extension test;
    // well-named DIRECTORIES would fill it if the cap counted listed entries
    // instead of accepted ones. The real picture has to survive both.
    const files: Record<string, Uint8Array> = { 'p999-01.png': png() };
    for (let i = 0; i < MAX_IMAGES; i++) files[`a&b${String(i).padStart(3, '0')}.png`] = png();
    const fountain = cached(files);
    for (let i = 1; i <= MAX_IMAGES; i++) {
      mkdirSync(join(imagesDirFor(fountain), `p${String(i).padStart(3, '0')}-01.png`));
    }
    const { images } = await readImages(fountain);
    expect([...images.keys()]).toEqual(['p999-01.png']);
  });
});

// The pictures a script's notes ask for (`imageNoteFiles`), handed to the
// read so it can say which of them the folder does not hold at all. Every
// lost picture is counted once, under the most specific cause the read knows:
// a listed name it refused is `refused`, a listed name past the cap is
// `overCap`, a folder that is a link or a file is `folderRefused`, and only a
// name the folder does not list is `notFound`.
describe('readImages, told which pictures the script asks for', () => {
  const asked = (...names: string[]) => new Set(names);

  test('a picture the folder does not list is not found', async () => {
    // The iCloud case: an evicted copy is renamed `.p002-01.png.icloud`,
    // which no note could name, so the listing never offers it as a picture.
    const fountain = cached({ 'p001-01.png': png(), '.p002-01.png.icloud': png() });
    const read = await readImages(fountain, asked('p001-01.png', 'p002-01.png'));
    expect([...read.images.keys()]).toEqual(['p001-01.png']);
    expect([read.refused, read.overCap, read.notFound]).toEqual([0, 0, 1]);
  });

  test('a folder that is not there holds none of them', async () => {
    const read = await readImages(cached(null), asked('p001-01.png', 'p002-01.png'));
    expect(read.notFound).toBe(2);
  });

  test('a listed picture the read refused is refused, and not also not found', async () => {
    const fountain = cached({ 'p001-01.png': new Uint8Array(0) });
    const read = await readImages(fountain, asked('p001-01.png', 'p002-01.png'));
    expect([read.refused, read.notFound]).toEqual([1, 1]);
  });

  test('a listed picture past the cap is over the cap, and not also not found', async () => {
    const many: Record<string, Uint8Array> = {};
    for (let i = 1; i <= MAX_IMAGES + 1; i++) many[`p${String(i).padStart(3, '0')}-01.png`] = png();
    const last = `p0${MAX_IMAGES + 1}-01.png`;
    const read = await readImages(cached(many), asked('p001-01.png', last));
    expect(read.images.has(last)).toBe(false);
    expect([read.overCap, read.notFound]).toEqual([1, 0]);
  });

  test('a folder that is a link says so once, and counts nothing as not found', async () => {
    const { fountain } = linkedFolder();
    const read = await readImages(fountain, asked('p009-01.png', 'p010-01.png'));
    expect(read.folderRefused).toBe(true);
    expect(read.notFound).toBe(0);
    expect(imageFolderWarnings(read)).toHaveLength(1);
  });

  test('asked for nothing, nothing is missing', async () => {
    expect((await readImages(cached(null))).notFound).toBe(0);
    expect((await readImages(cached({ 'p001-01.png': png() }))).notFound).toBe(0);
  });

  // The script's own pictures are what a warning about this book is about.
  // A stray file no note names costs the book nothing, so Tune must not say
  // a picture was left out because of one.
  test('a stray file no note names is not a refusal when the script says what it shows', async () => {
    const fountain = cached({
      'p001-01.png': png(1),
      'p002-01.png': png(2),
      'stray.png': new Uint8Array(0),
    });
    const read = await readImages(fountain, asked('p001-01.png', 'p002-01.png'));
    expect([...read.images.keys()]).toEqual(['p001-01.png', 'p002-01.png']);
    expect([read.refused, read.overCap, read.notFound]).toEqual([0, 0, 0]);
    expect(imageFolderWarnings(read)).toEqual([]);
  });

  test('nor is a stray past the cap', async () => {
    // Sixty good pictures fill the cap, the script's one picture among them;
    // the two strays after them are turned away and the book lacks nothing.
    const many: Record<string, Uint8Array> = {};
    for (let i = 1; i <= MAX_IMAGES; i++) many[`p${String(i).padStart(3, '0')}-01.png`] = png();
    many['x-stray-1.png'] = png();
    many['x-stray-2.png'] = png();
    const read = await readImages(cached(many), asked('p001-01.png'));
    expect(read.images.has('p001-01.png')).toBe(true);
    expect([read.refused, read.overCap]).toEqual([0, 0]);
  });

  test('a picture the script shows is still counted beside a stray that is not', async () => {
    const fountain = cached({
      'p001-01.png': new Uint8Array(0),
      'stray.png': new Uint8Array(0),
    });
    const read = await readImages(fountain, asked('p001-01.png'));
    expect(read.refused).toBe(1);
  });

  test('asked for nothing, every refused file still counts, strays included', async () => {
    // The Kindle rebuild reads the folder without saying what the script
    // shows (export/artifact.ts), and keeps today's count.
    const fountain = cached({ 'p001-01.png': png(), 'stray.png': new Uint8Array(0) });
    expect((await readImages(fountain)).refused).toBe(1);
    expect((await readImages(fountain, asked())).refused).toBe(1);
    const many: Record<string, Uint8Array> = {};
    for (let i = 1; i <= MAX_IMAGES; i++) many[`p${String(i).padStart(3, '0')}-01.png`] = png();
    many['x-stray-1.png'] = png();
    expect((await readImages(cached(many))).overCap).toBe(1);
  });
});

describe('imageFolderWarnings', () => {
  /** An `ImagesRead` with the counts under test. `kept` is a real map, since
   * the over-cap sentence adds it to `overCap` to say what the folder held. */
  const read = (o: { kept?: number; refused?: number; overCap?: number; notFound?: number }) => ({
    images: new Map(
      Array.from({ length: o.kept ?? 0 }, (_, i) => [`p${i}.png`, png()] as const),
    ),
    refused: o.refused ?? 0,
    overCap: o.overCap ?? 0,
    notFound: o.notFound ?? 0,
  });

  test('silence when the folder gave up everything it had', () => {
    expect(imageFolderWarnings(read({ kept: 3 }))).toEqual([]);
    expect(imageFolderWarnings(read({ refused: -1, overCap: -1 }))).toEqual([]);
  });

  test('one refusal is singular, and says where to look', () => {
    const [w] = imageFolderWarnings(read({ refused: 1 }));
    expect(w).toMatch(/^1 picture (?!s)/);
    expect(w).toContain('picture folder');
    expect(w).not.toContain('—');
    expect(w.endsWith('.')).toBe(true);
  });

  test('more than one is plural', () => {
    expect(imageFolderWarnings(read({ refused: 4 }))[0]).toMatch(/^4 pictures /);
  });

  test("a folder holding more than the book can carry says so, in the PDF path's own terms", () => {
    // Deliberately the same shape of sentence `imageWarnings` writes for
    // `ImageStats.overCap`: a filmmaker who converts a PDF and then re-renders
    // the .fountain must not be told two different stories about one ceiling.
    const [w] = imageFolderWarnings(read({ kept: MAX_IMAGES, overCap: 5 }));
    expect(w).toContain(`${MAX_IMAGES + 5} pictures`);
    expect(w).toContain(`first ${MAX_IMAGES}`);
    expect(w).toContain('picture folder');
    expect(w).not.toContain('—');
  });

  test('the two losses are separate sentences, and both are said', () => {
    // Rolling them together would tell someone their drive was missing when
    // their folder was merely full.
    const out = imageFolderWarnings(read({ kept: MAX_IMAGES, refused: 2, overCap: 3 }));
    expect(out).toHaveLength(2);
    expect(out.some((w) => w.includes('could not be read'))).toBe(true);
    expect(out.some((w) => w.includes(`${MAX_IMAGES + 3} pictures`))).toBe(true);
  });

  test('pictures the script shows and the folder does not hold are said, with the count', () => {
    expect(imageFolderWarnings(read({ notFound: 1 }))).toEqual([
      '1 picture this script shows could not be found in its picture folder, so the book was made without it.',
    ]);
    expect(imageFolderWarnings(read({ notFound: 2 }))).toEqual([
      '2 pictures this script shows could not be found in its picture folder, so the book was made without them.',
    ]);
  });

  test('a picture not found and a picture not read are separate sentences', () => {
    // One is a file that is not there, the other a file that is there and
    // is not a picture we can carry: different things to go and look at.
    const out = imageFolderWarnings(read({ refused: 1, notFound: 1 }));
    expect(out).toHaveLength(2);
    expect(out.some((w) => w.startsWith('1 picture could not be read'))).toBe(true);
    expect(out.some((w) => w.startsWith('1 picture this script shows could not be found'))).toBe(true);
    for (const w of out) expect(w).not.toContain('—');
  });
});

describe('pruneImages', () => {
  test('removes this writer\'s stale pictures and its abandoned temps', async () => {
    const fountain = cached({
      'p001-01.png': png(),
      'p009-09.png': png(),
      '.p002-01.png.999.tmp': png(),
    });
    const dir = imagesDirFor(fountain);
    expect(await pruneImages(dir, new Set(['p001-01.png']))).toBe(2);
    expect((await readdir(dir)).sort()).toEqual(['p001-01.png']);
  });

  test('never removes a picture a person put there', async () => {
    // THE data-loss case. Every one of these passes notes.ts's allowlist,
    // which is why the prune uses the writer's own shape instead.
    const kept = ['board-01.png', 'sunset.png', 'p1.png', 'A-01.png', 'readme.md'];
    const fountain = cached(Object.fromEntries(kept.map((n) => [n, png()])));
    const dir = imagesDirFor(fountain);
    expect(await pruneImages(dir, new Set())).toBe(0);
    expect((await readdir(dir)).sort()).toEqual([...kept].sort());
  });

  test('takes the folder away when it emptied it', async () => {
    const fountain = cached({ 'p001-01.png': png() });
    const dir = imagesDirFor(fountain);
    expect(await pruneImages(dir, new Set())).toBe(1);
    expect(await readdir(dir).catch(() => 'gone')).toBe('gone');
  });

  test('leaves a folder it did not empty, and one it did not touch', async () => {
    const a = imagesDirFor(cached({ 'p001-01.png': png(), 'readme.md': png() }));
    expect(await pruneImages(a, new Set())).toBe(1);
    expect(await readdir(a)).toEqual(['readme.md']);

    const b = imagesDirFor(cached({}));
    expect(await pruneImages(b, new Set())).toBe(0);
    expect(await readdir(b)).toEqual([]);
  });

  test('a missing folder is not an error and creates nothing', async () => {
    const dir = imagesDirFor(cached(null));
    expect(await pruneImages(dir, new Set())).toBe(0);
    expect(await readdir(dir).catch(() => 'gone')).toBe('gone');
  });

  test('never deletes through a symlinked folder', async () => {
    // The CLI calls this on its own for a script with no pictures left, so it
    // needs the folder check itself, not only through writeImages.
    const { dir, elsewhere } = linkedFolder();
    expect(await pruneImages(dir, new Set())).toBe(0);
    expect(await readdir(elsewhere)).toEqual(['p009-01.png']);
  });
});

describe('writeImages', () => {
  test('writes the pictures and leaves no temp behind', async () => {
    const dir = imagesDirFor(cached(null));
    await writeImages(dir, [
      { name: 'p001-01.png', bytes: png(4) },
      { name: 'p002-01.png', bytes: png(5) },
    ]);
    expect((await readdir(dir)).sort()).toEqual(['p001-01.png', 'p002-01.png']);
  });

  test('adopts a folder that is already there without taking anything out of it', async () => {
    const fountain = cached({ 'board-01.png': png(), 'p009-09.png': png() });
    const dir = imagesDirFor(fountain);
    await writeImages(dir, [{ name: 'p001-01.png', bytes: png() }]);
    // The user's picture stays; the last conversion's numbering goes.
    expect((await readdir(dir)).sort()).toEqual(['board-01.png', 'p001-01.png']);
  });

  test('a path already taken by a file refuses rather than writing anywhere else', async () => {
    const fountain = cached(null);
    writeFileSync(imagesDirFor(fountain), 'a file, not a folder');
    await expect(
      writeImages(imagesDirFor(fountain), [{ name: 'p001-01.png', bytes: png() }]),
    ).rejects.toThrow();
  });

  test('a symlinked folder is refused, and nothing lands in or leaves its target', async () => {
    // Adopting it would write THROUGH the link and then prune the link's
    // target: measured before this guard, the target gained the new picture
    // and lost its p009-01.png. ENOTDIR, which the CLI reports as
    // `unwritable` (cli-errors.ts), the same answer a FILE at this path gets.
    const { dir, elsewhere } = linkedFolder();
    await expect(writeImages(dir, [{ name: 'p001-01.png', bytes: png() }]))
      .rejects.toMatchObject({ code: 'ENOTDIR' });
    expect(await readdir(elsewhere)).toEqual(['p009-01.png']);
  });

  test('makes the folder itself and never its parents', async () => {
    // The folder sits beside a .fountain, so its parent is wherever that
    // .fountain goes. A parent that is not there is a path the caller got
    // wrong, and building it would put pictures somewhere nobody asked for.
    const base = join(SCRATCH, `case-${seq++}`);
    mkdirSync(base);
    const dir = join(base, 'missing', 'deeper', 'x.images');
    await expect(writeImages(dir, [{ name: 'p001-01.png', bytes: png() }]))
      .rejects.toMatchObject({ code: 'ENOENT' });
    expect(await readdir(base)).toEqual([]);
  });
});
