// bun tools/mobi-picture-probe.ts --out <folder>
//
// Builds small books for one question only a device can answer: how large
// a picture an older Kindle accepts in a .mobi. Writes five files into the
// folder, creating it if need be:
//
//   screepub-picture-probe.mobi         all three pictures in one book
//   screepub-picture-probe.epub         that book as an EPUB, to compare it with
//   screepub-picture-probe-2kb.mobi     the 2 KB picture alone
//   screepub-picture-probe-200kb.mobi   the 200 KB picture alone
//   screepub-picture-probe-1400kb.mobi  the 1.4 MB plate alone
//
// The four .mobi files all go to the Kindle's documents folder. The single
// books are there for the failure the combined one cannot report on: if one
// record too large makes the Kindle refuse the whole book, the combined book
// says only that something failed, and the smaller sizes still answer from
// books of their own. Each has its own title (Picture Probe 2 KB, and so on),
// so the library tells them apart.
//
// Every book comes from a .fountain text through convertFountain, the route a
// re-render from a .fountain takes, so the probe is built by the engine's own
// renderer and writer rather than a copy of them. The combined book shows the
// three pictures, each announced by a line of action that names its size:
// about 2 KB, about 200 KB, and about 1.4 MB as a full-page plate. The middle
// one is past the single-record cap older Kindle firmware is reported to have
// (about 127 KB); the last is the size of the largest full-page plate
// measured in a real script. Why a cap is plausible at all is in registry
// #19a's "On a device" bullet.
//
// Every run writes the same bytes: the pictures are drawn from seeded static,
// and each build runs with its clock fixed. So a finding on one copy holds
// for every copy. Prints the files and the three sizes. Exit codes: 0 done,
// 1 refused or failed.

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { convertFountain } from '../src/convert';
import { encodePng } from '../src/parser/png';

export const MOBI_FILE = 'screepub-picture-probe.mobi';
export const EPUB_FILE = 'screepub-picture-probe.epub';

/** One of the probe's pictures: the name its note gives it, and its PNG. */
export interface ProbePicture {
  name: string;
  bytes: Uint8Array;
  /** Set as a full-page plate, as the largest real plates are. */
  fullPage: boolean;
  /**
   * Its size as the single book that holds it alone is named, in the file
   * name and in the title. Fixed rather than measured, so the files are
   * called the same whatever a different zlib makes of the bytes; the line of
   * action inside each book is the measured size.
   */
  nominal: { file: string; title: string };
}

/** The file the single book holding `picture` alone is written to. */
export function singleMobiFile(picture: ProbePicture): string {
  return `screepub-picture-probe-${picture.nominal.file}.mobi`;
}

/** The title of the single book holding `picture` alone. */
export function singleTitle(picture: ProbePicture): string {
  return `Picture Probe ${picture.nominal.title}`;
}

/** mulberry32: a small seeded generator, so every run draws the same static. */
function seeded(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return (t ^ (t >>> 14)) >>> 0;
  };
}

/**
 * A black frame 8 px wide around a checkerboard of 40 px squares in two
 * greys, with a centred `staticW` x `staticH` rectangle of RGB static. The
 * static is what sets the size: it does not compress, so each pixel of it
 * costs about three bytes, while the frame and the checkerboard together
 * cost about a kilobyte. On a device that shows it, it is plainly a picture,
 * and on one that does not, there is nothing in its place to mistake for one.
 */
function drawPicture(width: number, height: number, staticW: number, staticH: number, seed: number): Uint8Array {
  const FRAME = 8;
  const SQUARE = 40;
  const next = seeded(seed);
  const x0 = Math.floor((width - staticW) / 2);
  const y0 = Math.floor((height - staticH) / 2);
  const rgb = new Uint8Array(width * height * 3);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 3;
      // The frame is black, which is the zeros the array starts with.
      if (x < FRAME || y < FRAME || x >= width - FRAME || y >= height - FRAME) continue;
      if (x >= x0 && x < x0 + staticW && y >= y0 && y < y0 + staticH) {
        const r = next();
        rgb[i] = r;
        rgb[i + 1] = r >>> 8;
        rgb[i + 2] = r >>> 16;
        continue;
      }
      const light = ((Math.floor(x / SQUARE) + Math.floor(y / SQUARE)) & 1) === 0;
      rgb[i] = rgb[i + 1] = rgb[i + 2] = light ? 0xe0 : 0x60;
    }
  }
  return encodePng(rgb, width, height);
}

/**
 * The three pictures, smallest first. Each rectangle of static was sized by
 * encoding it (2026-10-01): 13 x 13 makes 1,977 bytes, 256 x 255 makes
 * 199,905, and static across the whole 600 x 800 plate inside its frame makes
 * 1,381,847. A different zlib could move those a little, so the line of
 * action before each picture is written from its real size, and the test
 * holds each to a range rather than to a byte. The plate is 600 x 800, an
 * older Kindle's own screen, so its size in bytes is the only thing about it
 * that could be too large.
 */
export function probePictures(): ProbePicture[] {
  return [
    {
      name: 'p001-01.png', bytes: drawPicture(480, 360, 13, 13, 1), fullPage: false,
      nominal: { file: '2kb', title: '2 KB' },
    },
    {
      name: 'p002-01.png', bytes: drawPicture(480, 360, 256, 255, 2), fullPage: false,
      nominal: { file: '200kb', title: '200 KB' },
    },
    {
      name: 'p003-01.png', bytes: drawPicture(600, 800, 584, 784, 3), fullPage: true,
      nominal: { file: '1400kb', title: '1.4 MB' },
    },
  ];
}

/**
 * A size as a person reads one: whole kilobytes under a megabyte, megabytes
 * to one decimal above. Decimal units (1 KB is 1,000 bytes), as macOS shows
 * a file's size.
 */
export function plainSize(bytes: number): string {
  return bytes < 1_000_000 ? `${Math.round(bytes / 1000)} KB` : `${(bytes / 1_000_000).toFixed(1)} MB`;
}

/** The probe's script: a short scene per picture, each picture announced by
 * a line that names its size. Invented, like every fixture. The combined book
 * is all three under the title Picture Probe; a single book is one picture
 * under a title of its own. */
export function probeScript(pictures: readonly ProbePicture[], title = 'Picture Probe'): string {
  const scenes = ['INT. TEST ROOM - DAY', 'INT. LONG HALLWAY - NIGHT', 'EXT. ROOFTOP - DAWN'];
  const lines = [`Title: ${title}`, 'Author: Marlow Penn', ''];
  const one = pictures.length === 1;
  pictures.forEach((picture, i) => {
    lines.push(scenes[i], '');
    if (i === 0) {
      lines.push(
        one
          ? 'A plain room, kept for one question: whether this Kindle shows the picture in it.'
          : 'A plain room, kept for one question: which of three pictures this Kindle shows.',
        '',
        `${one ? 'The picture is' : 'Each picture is'} a black frame around a grey checkerboard `
          + 'with a patch of static in it. The bigger the file, the more of it is static.',
        '',
      );
    }
    const page = picture.fullPage ? ' It has a page to itself.' : '';
    lines.push(`The next picture is about ${plainSize(picture.bytes.length)}.${page}`, '');
    lines.push(`[[img: ${picture.name} w=${picture.fullPage ? '100 full' : '60'}]]`, '');
  });
  lines.push('The probe ends here.', '');
  return lines.join('\n');
}

/**
 * Runs `build` with the clock reading `at`. A book records when it was built
 * in three places, the MOBI's PDB header, the EPUB's dcterms:modified and
 * every zip entry's date, and each of them reads the clock through `Date`.
 * Fixing it for the length of one build is what makes two runs write the
 * same bytes; the real clock is put back however the build ends.
 */
async function atFixedClock<T>(at: number, build: () => Promise<T>): Promise<T> {
  const RealDate = globalThis.Date;
  class FixedDate extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(at);
      else super(...(args as [number]));
    }
    static now(): number {
      return at;
    }
  }
  globalThis.Date = FixedDate as DateConstructor;
  try {
    return await build();
  } finally {
    globalThis.Date = RealDate;
  }
}

/** The build time both books record, whenever they are built. */
const BUILT_AT = Date.UTC(2026, 9, 1);

/** One single-picture book: its file, its title, its picture and its .mobi. */
export interface SingleProbe {
  file: string;
  title: string;
  picture: ProbePicture;
  mobi: Uint8Array;
}

/** The books and the pictures they carry, built as the CLI builds a
 * .fountain's books with Screepub's own defaults (includeImages on): the
 * combined book as a .mobi and an EPUB, and each picture alone in a .mobi of
 * its own, smallest first. */
export async function buildProbe(): Promise<{
  mobi: Uint8Array;
  epub: Uint8Array;
  pictures: ProbePicture[];
  singles: SingleProbe[];
}> {
  const pictures = probePictures();
  const build = (shown: readonly ProbePicture[], title?: string) =>
    atFixedClock(BUILT_AT, () => convertFountain(probeScript(shown, title), {
      mobi: true,
      // Through the safety gate (MOBI_CARRIES_PICTURES in src/mobi/html.ts):
      // this probe is the device test that decides whether it opens.
      mobiPictures: true,
      images: new Map(shown.map((p) => [p.name, p.bytes])),
    }));
  const combined = await build(pictures);
  const singles: SingleProbe[] = [];
  for (const picture of pictures) {
    const title = singleTitle(picture);
    singles.push({ file: singleMobiFile(picture), title, picture, mobi: (await build([picture], title)).mobi! });
  }
  return { mobi: combined.mobi!, epub: combined.epub, pictures, singles };
}

if (import.meta.main) {
  try {
    const { values } = parseArgs({ args: Bun.argv.slice(2), options: { out: { type: 'string' } } });
    if (values.out === undefined) throw new Error('usage: bun tools/mobi-picture-probe.ts --out <folder>');
    const { mobi, epub, pictures, singles } = await buildProbe();
    mkdirSync(values.out, { recursive: true });
    writeFileSync(join(values.out, MOBI_FILE), mobi);
    writeFileSync(join(values.out, EPUB_FILE), epub);
    for (const single of singles) writeFileSync(join(values.out, single.file), single.mobi);
    const width = Math.max(MOBI_FILE.length, EPUB_FILE.length, ...singles.map((s) => s.file.length));
    console.log(`Wrote the probe to ${values.out}:`);
    console.log(`  ${MOBI_FILE.padEnd(width)}  Picture Probe, all three pictures`);
    for (const single of singles) console.log(`  ${single.file.padEnd(width)}  ${single.title}, that picture alone`);
    console.log(`  ${EPUB_FILE.padEnd(width)}  Picture Probe as an EPUB, to compare`);
    console.log("Copy the four .mobi files to the Kindle's documents folder.");
    console.log('The three pictures, in the order Picture Probe shows them:');
    pictures.forEach((p, i) => {
      const plate = p.fullPage ? ', a full-page plate' : '';
      console.log(`  ${i + 1}. about ${plainSize(p.bytes.length)} (${p.bytes.length.toLocaleString('en-US')} bytes)${plate}`);
    });
  } catch (e) {
    console.error(`mobi-picture-probe: ${e instanceof Error ? e.message : String(e)}`);
    process.exit(1);
  }
}
