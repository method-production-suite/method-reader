// bun tools/mobi-picture-probe.ts --out <folder>
//
// Builds a small book for one question only a device can answer: how large
// a picture an older Kindle accepts in a .mobi. Writes two files into the
// folder, creating it if need be:
//
//   screepub-picture-probe.mobi  the file to copy to the Kindle's documents folder
//   screepub-picture-probe.epub  the same book as an EPUB, to compare it with
//
// Both come from one .fountain text through convertFountain, the route a
// re-render from a .fountain takes, so the probe is built by the engine's own
// renderer and writer rather than a copy of them. The book shows three
// pictures, each announced by a line of action that names its size: about
// 2 KB, about 200 KB, and about 1.4 MB as a full-page plate. The middle one
// is past the single-record cap older Kindle firmware is reported to have
// (about 127 KB); the last is the size of the largest full-page plate
// measured in a real script. Why a cap is plausible at all is in registry
// #19a's "On a device" bullet.
//
// Every run writes the same bytes: the pictures are drawn from seeded static,
// and the build runs with its clock fixed. So a finding on one copy holds for
// every copy. Prints the three sizes. Exit codes: 0 done, 1 refused or failed.

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
    { name: 'p001-01.png', bytes: drawPicture(480, 360, 13, 13, 1), fullPage: false },
    { name: 'p002-01.png', bytes: drawPicture(480, 360, 256, 255, 2), fullPage: false },
    { name: 'p003-01.png', bytes: drawPicture(600, 800, 584, 784, 3), fullPage: true },
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

/** The probe's script: three short scenes, one picture each, each picture
 * announced by a line that names its size. Invented, like every fixture. */
export function probeScript(pictures: readonly ProbePicture[]): string {
  const scenes = ['INT. TEST ROOM - DAY', 'INT. LONG HALLWAY - NIGHT', 'EXT. ROOFTOP - DAWN'];
  const lines = ['Title: Picture Probe', 'Author: Marlow Penn', ''];
  pictures.forEach((picture, i) => {
    lines.push(scenes[i], '');
    if (i === 0) {
      lines.push(
        'A plain room, kept for one question: which of three pictures this Kindle shows.',
        '',
        'Each picture is a black frame around a grey checkerboard with a patch of static in it. '
          + 'The bigger the file, the more of it is static.',
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

/** Both books and the pictures they carry, built as the CLI builds a
 * .fountain's books with Screepub's own defaults (includeImages on). */
export async function buildProbe(): Promise<{ mobi: Uint8Array; epub: Uint8Array; pictures: ProbePicture[] }> {
  const pictures = probePictures();
  const images = new Map(pictures.map((p) => [p.name, p.bytes]));
  const result = await atFixedClock(BUILT_AT, () => convertFountain(probeScript(pictures), { mobi: true, images }));
  return { mobi: result.mobi!, epub: result.epub, pictures };
}

if (import.meta.main) {
  try {
    const { values } = parseArgs({ args: Bun.argv.slice(2), options: { out: { type: 'string' } } });
    if (values.out === undefined) throw new Error('usage: bun tools/mobi-picture-probe.ts --out <folder>');
    const { mobi, epub, pictures } = await buildProbe();
    mkdirSync(values.out, { recursive: true });
    writeFileSync(join(values.out, MOBI_FILE), mobi);
    writeFileSync(join(values.out, EPUB_FILE), epub);
    console.log(`Wrote the probe to ${values.out}:`);
    console.log(`  ${MOBI_FILE}  copy this one to the Kindle's documents folder`);
    console.log(`  ${EPUB_FILE}  the same book as an EPUB, to compare`);
    console.log('The three pictures, in the order the book shows them:');
    pictures.forEach((p, i) => {
      const plate = p.fullPage ? ', a full-page plate' : '';
      console.log(`  ${i + 1}. about ${plainSize(p.bytes.length)} (${p.bytes.length.toLocaleString('en-US')} bytes)${plate}`);
    });
  } catch (e) {
    console.error(`mobi-picture-probe: ${e instanceof Error ? e.message : String(e)}`);
    process.exit(1);
  }
}
