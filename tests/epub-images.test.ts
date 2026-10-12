// The pictures a book carries (registry #19): which ones go into the
// container, how they are stored, and how the package document lists them.
// The figure markup is tests/epub.test.ts's; this file starts where the body
// has already decided which pictures it shows.
import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseFountain } from '../src/fountain/parse';
import JSZip from 'jszip';
import { buildEpub, imageIds, type BookImage } from '../src/epub/build';
import { tokensToBody, tokensToPreviewHtml } from '../src/epub/html';
import { screenplayCss } from '../src/epub/css';
import { convertPdf } from '../src/convert';
import { DEFAULT_FORMAT_OPTIONS, resolveFormatOptions } from '../src/options';

const PUBLIC = new URL('./fixtures/', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-epub-images-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

// Enough of a PNG for everything below except epubcheck, which reads the
// picture: the signature, then bytes that tell one picture from another.
const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const png = (...tail: number[]) => new Uint8Array([...SIGNATURE, ...tail]);

const META = { title: 'T', identifier: 'urn:screepub:test' };

/** A body that shows each named picture once, in order, as tokensToBody
 * renders it for a caller that has them all. */
function bodyShowing(names: string[], format = DEFAULT_FORMAT_OPTIONS) {
  const notes = names.map((n) => `[[img: ${n} w=50]]\n\n`).join('');
  const tokens = parseFountain(`INT. LAB - DAY\n\nBefore.\n\n${notes}After.\n`).tokens;
  return tokensToBody(tokens, { format, images: new Set(names) });
}

async function open(bytes: Uint8Array) {
  const zip = await JSZip.loadAsync(bytes);
  const opf = await zip.file('OEBPS/package.opf')!.async('string');
  return { zip, opf };
}

// Files only: JSZip writes a folder entry for each directory a path implies,
// OEBPS/text/ included, so OEBPS/images/ appears beside the first picture.
const imageEntries = (zip: JSZip) =>
  Object.keys(zip.files).filter((n) => n.startsWith('OEBPS/images/') && !zip.files[n].dir);
const imageItems = (opf: string) => opf.match(/<item [^>]*media-type="image\/png"[^>]*\/>/g) ?? [];

/**
 * The compression method each entry's LOCAL header records (0 stored,
 * 8 deflated), read off the bytes rather than asked of JSZip, so the answer
 * is what a reading system's unzip will see.
 */
function methods(bytes: Uint8Array): Map<string, number> {
  const out = new Map<string, number>();
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i + 30 <= bytes.length && dv.getUint32(i, true) === 0x04034b50;) {
    const nameLen = dv.getUint16(i + 26, true);
    const name = new TextDecoder().decode(bytes.subarray(i + 30, i + 30 + nameLen));
    out.set(name, dv.getUint16(i + 8, true));
    i += 30 + nameLen + dv.getUint16(i + 28, true) + dv.getUint32(i + 18, true);
  }
  return out;
}

describe('the pictures a book carries', () => {
  test('a picture the body shows ships under its own name, bytes untouched, with its manifest item', async () => {
    const bytes = png(1, 2, 3);
    const { zip, opf } = await open(await buildEpub(
      META, bodyShowing(['p001-01.png']), DEFAULT_FORMAT_OPTIONS, [{ name: 'p001-01.png', bytes }],
    ));
    expect(imageEntries(zip)).toEqual(['OEBPS/images/p001-01.png']);
    expect([...await zip.file('OEBPS/images/p001-01.png')!.async('uint8array')]).toEqual([...bytes]);
    expect(imageItems(opf)).toEqual([
      '<item id="img-p001-01" href="images/p001-01.png" media-type="image/png"/>',
    ]);
  });

  test('a picture the body does not show is not shipped, however it was passed', async () => {
    // Dead weight in a format where weight is the whole problem, and no
    // validator will mention it: epubcheck is clean about an extra picture,
    // manifested or not (measured 2026-08-11), so this filter is the only
    // thing that catches it.
    const { zip, opf } = await open(await buildEpub(
      META, bodyShowing(['p001-01.png']), DEFAULT_FORMAT_OPTIONS,
      [{ name: 'p009-01.png', bytes: png(9) }, { name: 'p001-01.png', bytes: png(1) }],
    ));
    expect(imageEntries(zip)).toEqual(['OEBPS/images/p001-01.png']);
    expect(opf).not.toContain('p009-01');
  });

  test('a body that shows no picture gets no picture, whatever the caller holds', async () => {
    const plain = tokensToBody(parseFountain('INT. LAB - DAY\n\nBefore.\n').tokens);
    const { zip, opf } = await open(await buildEpub(
      META, plain, DEFAULT_FORMAT_OPTIONS, [{ name: 'p001-01.png', bytes: png(1) }],
    ));
    expect(imageEntries(zip)).toEqual([]);
    expect(imageItems(opf)).toEqual([]);
  });

  test('a book without pictures is the book it was: same entries, same bytes, same manifest', async () => {
    // Passing pictures nobody shows must change nothing, and the manifest
    // seam the picture items join is pinned line for line: a stray newline
    // there would land in every book.
    const plain = tokensToBody(parseFountain('INT. LAB - DAY\n\nBefore.\n').tokens);
    const strip = (s: string) => s.replace(/<meta property="dcterms:modified">[^<]*<\/meta>/, '');
    const a = await JSZip.loadAsync(await buildEpub(META, plain));
    const b = await JSZip.loadAsync(await buildEpub(
      META, plain, DEFAULT_FORMAT_OPTIONS, [{ name: 'p001-01.png', bytes: png(1) }],
    ));
    expect(Object.keys(b.files)).toEqual(Object.keys(a.files));
    for (const name of Object.keys(a.files)) {
      if (a.files[name].dir) continue;
      expect(strip(await b.file(name)!.async('string')), name)
        .toBe(strip(await a.file(name)!.async('string')));
    }
    const opf = await a.file('OEBPS/package.opf')!.async('string');
    expect(opf).toContain(
      '  <manifest>\n'
      + '    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>\n'
      + '    <item id="css" href="style.css" media-type="text/css"/>\n'
      + '    <item id="titlepage" href="titlepage.xhtml" media-type="application/xhtml+xml"/>\n'
      + '    <item id="body001" href="text/body001.xhtml" media-type="application/xhtml+xml"/>\n'
      + '  </manifest>\n',
    );
  });

  test('a name passed twice ships once, the first bytes with it', async () => {
    // Two manifest items for one file is OPF-074; two zip entries under one
    // path is a container some unzips refuse.
    const bytes = await buildEpub(
      META, bodyShowing(['p001-01.png']), DEFAULT_FORMAT_OPTIONS,
      [{ name: 'p001-01.png', bytes: png(1) }, { name: 'p001-01.png', bytes: png(2) }],
    );
    const { zip, opf } = await open(bytes);
    expect(imageItems(opf)).toHaveLength(1);
    expect([...methods(bytes).keys()].filter((n) => n === 'OEBPS/images/p001-01.png')).toHaveLength(1);
    expect([...await zip.file('OEBPS/images/p001-01.png')!.async('uint8array')]).toEqual([...png(1)]);
  });

  test('a picture the body shows but nobody handed over is a caller bug, and says so', async () => {
    // The `<img>` is already in the body, so leaving the file out would ship
    // an invalid book (RSC-007) that looks fine until a reader opens it.
    await expect(buildEpub(META, bodyShowing(['p001-01.png', 'p002-01.png']), DEFAULT_FORMAT_OPTIONS, [
      { name: 'p001-01.png', bytes: png(1) },
    ])).rejects.toThrow(/p002-01\.png/);
    await expect(buildEpub(META, bodyShowing(['p001-01.png']))).rejects.toThrow(/p001-01\.png/);
  });

  test('pictures are stored, not deflated, and everything else still deflates', async () => {
    // A PNG is already deflated: deflating sixty plates again saved 0.3% for
    // about 330 ms (measured 2026-08-11), and Tune rebuilds the book on every
    // move.
    const bytes = await buildEpub(
      META, bodyShowing(['p001-01.png', 'p002-01.png']), DEFAULT_FORMAT_OPTIONS,
      [{ name: 'p001-01.png', bytes: png(1) }, { name: 'p002-01.png', bytes: png(2) }],
    );
    const m = methods(bytes);
    expect(m.get('OEBPS/images/p001-01.png')).toBe(0);
    expect(m.get('OEBPS/images/p002-01.png')).toBe(0);
    expect(m.get('mimetype')).toBe(0);
    expect(m.get('OEBPS/text/body001.xhtml')).toBe(8);
    expect(m.get('OEBPS/package.opf')).toBe(8);
  });

  test('a picture is a resource: manifested, never in the spine, no properties', async () => {
    const { opf } = await open(await buildEpub(
      META, bodyShowing(['p001-01.png']), DEFAULT_FORMAT_OPTIONS, [{ name: 'p001-01.png', bytes: png(1) }],
    ));
    expect(imageItems(opf)).toHaveLength(1);
    const spine = /<spine>([\s\S]*)<\/spine>/.exec(opf)![1];
    expect(spine).not.toContain('img');
    expect(imageItems(opf).join('')).not.toContain('properties=');
  });

  test('the manifest href and the figure src name the same zip entry', async () => {
    // The package document sits at OEBPS/ and the body at OEBPS/text/, so
    // one file is spelled two ways. Resolved, not reasoned about: one wrong
    // `../` is RSC-007 on one side and a broken picture on the other.
    const { zip, opf } = await open(await buildEpub(
      META, bodyShowing(['p001-01.png']), DEFAULT_FORMAT_OPTIONS, [{ name: 'p001-01.png', bytes: png(1) }],
    ));
    const href = /<item id="img-[^"]*" href="([^"]+)"/.exec(opf)![1];
    const xhtml = await zip.file('OEBPS/text/body001.xhtml')!.async('string');
    const src = /<img src="([^"]+)"/.exec(xhtml)![1];
    const resolve = (ref: string, from: string) => new URL(ref, `file:///${from}`).pathname.slice(1);
    expect(resolve(href, 'OEBPS/package.opf')).toBe('OEBPS/images/p001-01.png');
    expect(resolve(src, 'OEBPS/text/body001.xhtml')).toBe('OEBPS/images/p001-01.png');
  });

  test('the stylesheet carries the figure rules exactly when the book ships a picture', async () => {
    const format = resolveFormatOptions({ elementSpacingEm: 0.6 });
    const shown = await JSZip.loadAsync(await buildEpub(
      META, bodyShowing(['p001-01.png'], format), format, [{ name: 'p001-01.png', bytes: png(1) }],
    ));
    expect(await shown.file('OEBPS/style.css')!.async('string'))
      .toBe(screenplayCss(format, { hasFigures: true }));
    const plain = tokensToBody(parseFountain('INT. LAB - DAY\n\nBefore.\n').tokens, { format });
    const none = await JSZip.loadAsync(await buildEpub(
      META, plain, format, [{ name: 'p001-01.png', bytes: png(1) }],
    ));
    expect(await none.file('OEBPS/style.css')!.async('string')).toBe(screenplayCss(format));
  });
});

// ── the cover ────────────────────────────────────────────

// The title card as the cover: the body leaves it out of the flow and names it
// (tests/epub.test.ts), and the packager ships it and says it is the cover.
describe('the cover in the package', () => {
  const CARD = '[[img: p001-01.png w=100 full]]\n\n';
  const SCRIPT = 'INT. LAB - DAY\n\nWork.\n';
  const coverBody = (src = CARD + SCRIPT, format = DEFAULT_FORMAT_OPTIONS, names = ['p001-01.png']) =>
    tokensToBody(parseFountain(src).tokens, { format, images: new Set(names) });
  const metadataOf = (opf: string) => /<metadata[^>]*>([\s\S]*)<\/metadata>/.exec(opf)![1];

  test('a cover no page draws still ships: its bytes, its manifest item, and both declarations', async () => {
    // The hazard: the cover is on no page, so nothing in the body points at
    // it. A packager that shipped only what the pages draw would drop its
    // bytes, then its manifest item, then the declarations, and the book
    // would still validate and still read, with a grey rectangle in the
    // library. Every one of the three is asserted here for that reason.
    const bytes = png(7);
    const body = coverBody();
    expect(body.coverImage).toBe('p001-01.png');
    expect(body.usedImages.size).toBe(0);
    const { zip, opf } = await open(await buildEpub(
      META, body, DEFAULT_FORMAT_OPTIONS, [{ name: 'p001-01.png', bytes }],
    ));
    expect(imageEntries(zip)).toEqual(['OEBPS/images/p001-01.png']);
    expect([...await zip.file('OEBPS/images/p001-01.png')!.async('uint8array')]).toEqual([...bytes]);
    expect(imageItems(opf)).toEqual([
      '<item id="img-p001-01" href="images/p001-01.png" media-type="image/png" properties="cover-image"/>',
    ]);
    expect(metadataOf(opf)).toContain('<meta name="cover" content="img-p001-01"/>');
    // A resource like every picture, never in the spine.
    expect(/<spine>([\s\S]*)<\/spine>/.exec(opf)![1]).not.toContain('img');
  });

  test('one item is the cover-image, and the meta names that same item', async () => {
    const src = `${CARD}INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nWork.\n`;
    const body = coverBody(src, DEFAULT_FORMAT_OPTIONS, ['p001-01.png', 'p002-01.png']);
    const { opf } = await open(await buildEpub(META, body, DEFAULT_FORMAT_OPTIONS, [
      { name: 'p002-01.png', bytes: png(2) },
      { name: 'p001-01.png', bytes: png(1) },
    ]));
    expect(imageItems(opf)).toHaveLength(2);
    const covers = imageItems(opf).filter((item) => item.includes('properties='));
    expect(covers).toHaveLength(1);
    expect(covers[0]).toContain('href="images/p001-01.png"');
    expect(covers[0]).toContain('properties="cover-image"');
    const id = /id="([^"]+)"/.exec(covers[0])![1];
    expect(opf.match(/<meta name="cover"[^>]*\/>/g)).toEqual([`<meta name="cover" content="${id}"/>`]);
  });

  test('a book without a cover declares none', async () => {
    const off = resolveFormatOptions({ promoteCoverImage: false });
    for (const body of [bodyShowing(['p001-01.png']), coverBody(CARD + SCRIPT, off)]) {
      expect(body.coverImage).toBeUndefined();
      const { opf } = await open(await buildEpub(
        META, body, DEFAULT_FORMAT_OPTIONS, [{ name: 'p001-01.png', bytes: png(1) }],
      ));
      expect(opf).not.toContain('name="cover"');
      expect(opf).not.toContain('cover-image');
    }
  });

  test('a book whose one picture is its cover carries no figure rules', async () => {
    // No page draws a figure, so the sheet is the one a book without
    // pictures gets: the rules follow what the pages show, not what ships.
    const format = resolveFormatOptions({ elementSpacingEm: 0.6 });
    const { zip } = await open(await buildEpub(
      META, coverBody(CARD + SCRIPT, format), format, [{ name: 'p001-01.png', bytes: png(1) }],
    ));
    expect(await zip.file('OEBPS/style.css')!.async('string')).toBe(screenplayCss(format));
  });

  test('a cover nobody handed over is a caller bug, and says so', async () => {
    // The same throw as a figure's, reached through the cover alone: no
    // page draws this picture, so only the cover asks for its bytes.
    const body = coverBody();
    expect(body.usedImages.size).toBe(0);
    await expect(buildEpub(META, body, DEFAULT_FORMAT_OPTIONS, []))
      .rejects.toThrow(/p001-01\.png/);
  });
});

describe('picture ids', () => {
  test('img- plus the name without its extension, reduced to letters, digits, _ and -', () => {
    // The prefix is load-bearing: an XML id may not start with a digit, and
    // `9-plate.png` clears the filename allowlist.
    expect(imageIds(['p001-01.png', '9-plate.png', 'a_b.png', 'x.y.z.png'], new Set()))
      .toEqual(['img-p001-01', 'img-9-plate', 'img-a_b', 'img-x-y-z']);
  });

  test('names that reduce alike are bumped -2, -3 against every id already taken', () => {
    // `.` is inside the filename allowlist and outside the id class, so
    // `a.b.png` and `a-b.png` both want img-a-b. Two items sharing an id is
    // RSC-005 twice plus RSC-008 for the item it displaced (measured
    // 2026-08-11). The bump is checked against everything taken, so a folder
    // that also holds `a-b-2.png` is bumped again rather than colliding.
    expect(imageIds(['a.b.png', 'a-b.png', 'a-b-2.png'], new Set()))
      .toEqual(['img-a-b', 'img-a-b-2', 'img-a-b-2-2']);
    expect(imageIds(['a-b-2.png', 'a.b.png', 'a-b.png'], new Set()))
      .toEqual(['img-a-b-2', 'img-a-b', 'img-a-b-3']);
  });

  test('the ids the package document already uses are taken too', () => {
    expect(imageIds(['cover.png', 'cover.png'], new Set(['img-cover'])))
      .toEqual(['img-cover-2', 'img-cover-3']);
  });

  test('every id in a built package document is unique and a legal XML name', async () => {
    const names = ['a.b.png', 'a-b.png', 'a-b-2.png', '9-plate.png', 'p001-01.png'];
    const { zip, opf } = await open(await buildEpub(
      META, bodyShowing(names), DEFAULT_FORMAT_OPTIONS,
      names.map((name, i) => ({ name, bytes: png(i) })),
    ));
    const ids = [...opf.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]);
    expect(ids).toContain('uid');
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(id).toMatch(/^[A-Za-z_][A-Za-z0-9_.-]*$/);
    expect(imageItems(opf)).toHaveLength(5);
    expect(imageEntries(zip).sort()).toEqual(names.map((n) => `OEBPS/images/${n}`).sort());
  });
});

// ── the reader preview ───────────────────────────────────

// The preview is one self-contained document the window hands an iframe as
// text, so there is no container for a figure's path to point into. It
// carries each picture inside the `<img>` instead, and nothing else about the
// figure differs from the book's.
describe('the preview carries the pictures it shows', () => {
  // images.pdf's own .fountain and the pictures its conversion carried: the
  // pair the window's re-render reads back from the library.
  let fountainText = '';
  let pictures: ReadonlyMap<string, Uint8Array> = new Map();
  beforeAll(async () => {
    const result = await convertPdf(new Uint8Array(await Bun.file(`${PUBLIC}images.pdf`).arrayBuffer()));
    fountainText = result.fountainText;
    pictures = new Map(result.images.map((img) => [img.name, img.bytes]));
  }, 60000);
  const tokens = () => parseFountain(fountainText).tokens;
  const DATA = 'data:image/png;base64,';

  // The title card left in the flow, so both pictures are figures here; the
  // cover's effect on the preview is its own test below.
  const NO_COVER = resolveFormatOptions({ promoteCoverImage: false });

  test('each figure inlines its picture as a data: URI that decodes to the very bytes', () => {
    const html = tokensToPreviewHtml(tokens(), NO_COVER, pictures);
    const srcs = [...html.matchAll(/<img src="([^"]+)"/g)].map((m) => m[1]);
    expect(srcs).toHaveLength(2);
    expect([...pictures.keys()]).toEqual(['p001-01.png', 'p002-01.png']);
    srcs.forEach((src, i) => {
      expect(src.startsWith(DATA)).toBe(true);
      const decoded = Buffer.from(src.slice(DATA.length), 'base64');
      expect([...decoded]).toEqual([...pictures.get(`p00${i + 1}-01.png`)!]);
    });
  });

  test('it is the book’s own body, with each src pointed at the bytes and the sheet inlined', () => {
    // One markup path. Put each data: URI back to the book's path and the
    // preview is the body file the book would hold, figure rules and all, so
    // a figure cannot look one way in the window and another on a reader.
    const format = resolveFormatOptions({ elementSpacingEm: 0.6 });
    const html = tokensToPreviewHtml(tokens(), format, pictures);
    const named = new Map([...pictures].map(([name, bytes]) => [Buffer.from(bytes).toString('base64'), name]));
    const asBook = html.replace(
      /src="data:image\/png;base64,([^"]*)"/g,
      (_, b64: string) => `src="../images/${named.get(b64)}"`,
    );
    const book = tokensToBody(tokens(), {
      format, images: new Set(pictures.keys()), maxFileBytes: Number.MAX_SAFE_INTEGER,
    }).files[0].xhtml;
    expect(asBook).toBe(book.replace(
      /<link rel="stylesheet"[^>]*\/>/,
      () => `<style>\n${screenplayCss(format, { hasFigures: true })}</style>`,
    ));
    expect(asBook).not.toContain(DATA);
  });

  test('with the title card as the cover, the preview leaves it out, as the book’s pages do', () => {
    // The preview is the book's body, and the cover is on none of its pages.
    // So by default it opens on the first scene (sc-001 is the first section
    // id, so no Opening stands ahead of it), and the panel is its only
    // picture.
    const html = tokensToPreviewHtml(tokens(), DEFAULT_FORMAT_OPTIONS, pictures);
    const srcs = [...html.matchAll(/<img src="([^"]+)"/g)].map((m) => m[1]);
    expect(srcs).toEqual([`${DATA}${Buffer.from(pictures.get('p002-01.png')!).toString('base64')}`]);
    expect(html).toContain(
      '<section class="scene" id="sc-001">\n<h2 class="scene-heading">INT. ARCHIVE ROOM - DAY</h2>',
    );
  });

  test('with includeImages off, or no pictures in hand, the preview is the one it always was', () => {
    // Off: the pictures in hand change nothing. None in hand, or an empty
    // map, is the two-argument call every caller made before pictures
    // existed, and all four are the same document to the byte: no figure,
    // no data:, and a stylesheet without the figure rules.
    const on = DEFAULT_FORMAT_OPTIONS;
    const off = resolveFormatOptions({ includeImages: false });
    const plain = tokensToPreviewHtml(tokens(), on);
    expect(tokensToPreviewHtml(tokens(), on, new Map())).toBe(plain);
    expect(tokensToPreviewHtml(tokens(), off, pictures)).toBe(plain);
    expect(tokensToPreviewHtml(tokens(), off)).toBe(plain);
    expect(plain).not.toContain('class="figure');
    expect(plain).not.toContain('data:');
    expect(plain).toContain(`<style>\n${screenplayCss(on)}</style>`);
  });
});

// ── epubcheck ────────────────────────────────────────────

const EPUBCHECK = Bun.which('epubcheck');
const checked = EPUBCHECK ? test : test.skip;
const SKIPPED = EPUBCHECK ? '' : ' (skipped: epubcheck is not on PATH; brew install epubcheck)';

function epubcheck(path: string): { ok: boolean; out: string } {
  const run = Bun.spawnSync([EPUBCHECK!, path], { stdout: 'pipe', stderr: 'pipe' });
  const out = `${run.stdout.toString()}\n${run.stderr.toString()}`;
  return { ok: run.exitCode === 0 && out.includes('No errors or warnings detected'), out };
}

describe('epubcheck', () => {
  // images.pdf through the whole pipeline: the book convertPdf builds, with
  // the pictures it carried. The collision case below reuses those pictures
  // under hand-made names, rendered and packaged here directly.
  let pictures: BookImage[] = [];
  let book: Uint8Array = new Uint8Array();
  let title = '';
  beforeAll(async () => {
    if (!EPUBCHECK) return;
    const result = await convertPdf(new Uint8Array(await Bun.file(`${PUBLIC}images.pdf`).arrayBuffer()));
    pictures = result.images;
    book = result.epub;
    title = result.meta.title;
  }, 60000);

  const build = async (images: BookImage[], text: string) => {
    const tokens = parseFountain(text).tokens;
    const body = tokensToBody(tokens, { images: new Set(images.map((i) => i.name)) });
    return { body, epub: await buildEpub({ ...META, title }, body, DEFAULT_FORMAT_OPTIONS, images) };
  };

  checked(`the book convertPdf builds from images.pdf validates clean, cover declared${SKIPPED}`, async () => {
    const { zip, opf } = await open(book);
    expect(imageEntries(zip)).toEqual(['OEBPS/images/p001-01.png', 'OEBPS/images/p002-01.png']);
    // The default book declares its title card as the cover, both ways, so
    // this is epubcheck's verdict on the declarations as well as the pictures.
    expect(opf).toContain('<meta name="cover" content="img-p001-01"/>');
    expect(opf).toContain('href="images/p001-01.png" media-type="image/png" properties="cover-image"');
    const path = join(SCRATCH, 'images.epub');
    writeFileSync(path, book);
    const { ok, out } = epubcheck(path);
    expect(ok, out).toBe(true);
  }, 120000);

  checked(`the book convertPdf builds from torture.pdf validates clean, cover declared${SKIPPED}`, async () => {
    // The device pass's book: a title card in front and three pictures in
    // the flow, two of them full-page plates that are not the cover.
    const result = await convertPdf(new Uint8Array(await Bun.file(`${PUBLIC}torture.pdf`).arrayBuffer()));
    const { opf } = await open(result.epub);
    expect(opf).toContain('<meta name="cover" content="img-p001-01"/>');
    expect(opf.match(/properties="cover-image"/g)).toHaveLength(1);
    const path = join(SCRATCH, 'torture.epub');
    writeFileSync(path, result.epub);
    const { ok, out } = epubcheck(path);
    expect(ok, out).toBe(true);
  }, 120000);

  checked(`names that reduce to the same id still validate clean${SKIPPED}`, async () => {
    // The collision case end to end, with real pictures under hand-made
    // names, since only a folder somebody else filled can hold this pair.
    const renamed = [
      { name: 'a.b.png', bytes: pictures[0].bytes },
      { name: 'a-b.png', bytes: pictures[1].bytes },
    ];
    const text = 'INT. LAB - DAY\n\nBefore.\n\n[[img: a.b.png w=40]]\n\n'
      + 'Between.\n\n[[img: a-b.png w=100 full]]\n\nAfter.\n';
    const { body, epub } = await build(renamed, text);
    expect(body.usedImages.size).toBe(2);
    const path = join(SCRATCH, 'collision.epub');
    writeFileSync(path, epub);
    const { ok, out } = epubcheck(path);
    expect(ok, out).toBe(true);
  }, 120000);
});
