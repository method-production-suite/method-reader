// Screepub's own AZW3, read back by the structural reader in
// tests/helpers/kf8-reader.ts: no Calibre needed, so this is what CI proves.
// tests/azw3-calibre.test.ts reads the same books with Calibre's reader.
import { describe, expect, test } from 'bun:test';
import { buildAzw3 } from '../src/azw3/writer';
import { readEpub } from '../src/azw3/epub';
import { parseXml, textContent, findAll, getAttr } from '../src/azw3/xml';
import { exthNumber, exthString, exthValue, readKf8, type Kf8Book } from './helpers/kf8-reader';
import { fixtureEpub, type FixtureName } from './helpers/fixture-epubs';
import { maskPdbDates } from './helpers/mobi-records';

const books = new Map<FixtureName, Promise<{ epub: Uint8Array; azw3: Uint8Array; book: Kf8Book }>>();
function built(name: FixtureName) {
  let p = books.get(name);
  if (!p) {
    p = (async () => {
      const epub = await fixtureEpub(name);
      const azw3 = await buildAzw3(epub);
      return { epub, azw3, book: readKf8(azw3) };
    })();
    books.set(name, p);
  }
  return p;
}

const norm = (s: string) => s.replace(/\s+/g, ' ').trim();
/** The body text of each spine document, in order. */
async function epubTexts(epub: Uint8Array): Promise<string[]> {
  const book = await readEpub(epub);
  return book.spine.map((s) => norm(textContent(findAll(s.doc.root, 'body')[0])));
}
const azw3Texts = (book: Kf8Book) => book.parts.map((p) => norm(textContent(findAll(parseXml(p).root, 'body')[0])));

describe('our AZW3, read back structurally', () => {
  for (const name of ['field-station', 'images', 'torture', 'screenplay'] as const) {
    test(`${name}: a well-formed KF8 file that carries every word, in order`, async () => {
      const { epub, book } = await built(name);
      expect(book.header.version).toBe(8);
      expect(book.header.compression).toBe(2);
      expect(book.header.bookType).toBe(2);
      expect(book.header.extraDataFlags).toBe(0b11);
      expect(azw3Texts(book)).toEqual(await epubTexts(epub));
      // Every rebuilt file is whole XHTML with its stylesheet linked as a flow.
      for (const p of book.parts) expect(p).toContain('href="kindle:flow:0001?mime=text/css"');
      expect(book.flows.length).toBe(2);
      expect(new TextDecoder().decode(book.flows[1])).toContain('.dialogue-block');
      // Each text record's trailing data: an overlap that completes a
      // character, and a TBS entry.
      expect(book.trailing.every((t) => t.overlap.length <= 3 && t.tbs !== null)).toBe(true);
    });
  }

  test('the stylesheet travels verbatim, keeps and all (registry #8b)', async () => {
    const { epub, book } = await built('field-station');
    const css = await (await import('jszip')).default.loadAsync(epub).then((z) => z.file('OEBPS/style.css')!.async('string'));
    expect(new TextDecoder().decode(book.flows[1])).toBe(css);
    expect(css).toContain('break-after: avoid');
  });

  test('metadata: title, author, language, the ASIN and EBOK', async () => {
    const { epub, book } = await built('field-station');
    const e = await readEpub(epub);
    expect(book.header.title).toBe(e.title);
    expect(exthString(book, 503)).toEqual([e.title]);
    expect(exthString(book, 100)).toEqual(e.authors);
    expect(exthString(book, 524)).toEqual(['en']);
    expect(book.header.language).toBe(9);
    expect(exthString(book, 501)).toEqual(['EBOK']);
    expect(exthString(book, 113)).toEqual([e.identifier!.split(':').at(-1)!]);
    expect(exthString(book, 528)).toEqual(['true']);
  });

  test('the TOC: one NCX entry per scene, in order, each pointing at its scene', async () => {
    const { epub, book } = await built('field-station');
    const e = await readEpub(epub);
    expect(book.ncx.map((n) => n.label)).toEqual(e.toc.map((t) => t.title));
    expect(book.ncx.every((n) => n.depth === 0)).toBe(true);
    const rebuilt = new TextEncoder().encode(book.parts.join(''));
    e.toc.forEach((t, i) => {
      const n = book.ncx[i];
      // The offset is in the rebuilt text and lands on the scene's section.
      const tag = new TextDecoder().decode(rebuilt.slice(n.offset, n.offset + 200)).split('>')[0];
      expect(tag).toContain(`id="${t.href.split('#')[1]}"`);
      // pos:fid names a fragment and an offset that reach the same place.
      const chunk = book.chunks[n.pos];
      expect(chunk.insertPos + n.fid).toBe(n.offset);
      if (i > 0) expect(n.offset).toBeGreaterThan(book.ncx[i - 1].offset);
      expect(n.length).toBeGreaterThan(0);
    });
  });

  test('the guide starts reading at the body, past the title page', async () => {
    const { book } = await built('field-station');
    expect(book.guide.map((g) => g.type)).toEqual(['text']);
    const start = exthNumber(book, 116);
    expect(start.length).toBe(1);
    const rebuilt = new TextEncoder().encode(book.parts.join(''));
    expect(new TextDecoder().decode(rebuilt.slice(start[0], start[0] + 5))).toBe('<body');
    expect(start[0]).toBeGreaterThan(book.skel[0].start + book.skel[0].length);
  });

  test('the title page is the first file, and scene sections survive', async () => {
    const { book } = await built('field-station');
    expect(book.parts[0]).toContain('class="titlepage"');
    expect(book.parts.slice(1).join('')).toMatch(/<section class="scene" id="sc-001" aid="[0-9A-V]+">/);
  });

  test('pictures: every one a resource record, shown by kindle:embed, with the cover in EXTH 201', async () => {
    const { epub, book } = await built('images');
    const e = await readEpub(epub);
    expect(book.resources.length).toBe(e.images.length);
    e.images.forEach((img, i) => expect(Array.from(book.resources[i])).toEqual(Array.from(img.bytes)));
    expect(exthNumber(book, 125)).toEqual([e.images.length]);
    const cover = e.images.findIndex((i) => i.href === e.coverHref);
    expect(cover).toBeGreaterThanOrEqual(0);
    expect(exthNumber(book, 201)).toEqual([cover]);
    expect(exthNumber(book, 203)).toEqual([0]);
    // The pictures the pages show point at their records.
    const shown = book.parts.join('').match(/kindle:embed:([0-9A-V]{4})\?mime=image\/png/g) ?? [];
    const body = await readEpub(epub);
    const imgs = body.spine.flatMap((s) => findAll(s.doc.root, 'img').map((i) => getAttr(i, 'src')!));
    expect(shown.length).toBe(imgs.length);
  });

  test('a book with no pictures has no resources and no cover', async () => {
    const { book } = await built('field-station');
    expect(book.header.firstResource).toBe(0xffffffff);
    expect(exthValue(book, 201)).toEqual([]);
    expect(exthValue(book, 131)).toEqual([]);
  });

  test('two builds of one EPUB are the same file but for the PalmDB dates', async () => {
    const epub = await fixtureEpub('screenplay');
    expect(maskPdbDates(await buildAzw3(epub))).toEqual(maskPdbDates(await buildAzw3(epub)));
  });
});
