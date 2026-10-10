import { describe, expect, test } from 'bun:test';
import { CHUNK_SIZE, kf8Markup, idKey } from '../src/azw3/chunk';
import { readEpub, type EpubBook } from '../src/azw3/epub';
import { parseXml, textContent } from '../src/azw3/xml';
import { fixtureEpub } from './helpers/fixture-epubs';

const dec = new TextDecoder();

/** Rebuild each file from flow 0 and the two tables, as a reader does. */
function rebuildFiles(m: ReturnType<typeof kf8Markup>): string[] {
  const text = m.flows[0];
  let c = 0;
  return m.skelTable.map((s) => {
    let file = text.slice(s.startPos, s.startPos + s.length);
    let base = s.startPos + s.length;
    for (let i = 0; i < s.chunkCount; i++, c++) {
      const chunk = m.chunkTable[c];
      const part = text.slice(base, base + chunk.length);
      const at = chunk.insertPos - s.startPos;
      const next = new Uint8Array(file.length + part.length);
      next.set(file.slice(0, at));
      next.set(part, at);
      next.set(file.slice(at), at + part.length);
      file = next;
      base += chunk.length;
    }
    return dec.decode(file);
  });
}

function bookOf(bodies: string[], css = 'p { margin: 0 }'): EpubBook {
  return {
    title: 'T', authors: [], language: 'en',
    spine: bodies.map((b, i) => ({
      href: `text/f${i}.xhtml`,
      doc: parseXml(`<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">\n<head>\n<title>T</title>\n<link rel="stylesheet" type="text/css" href="../style.css"/>\n</head>\n<body>\n${b}</body>\n</html>\n`),
    })),
    stylesheets: new Map([['style.css', css]]),
    images: [], toc: [], landmarks: [],
  };
}

function checkMarkup(book: EpubBook, original: string[]) {
  const m = kf8Markup(book, new Map());
  const files = rebuildFiles(m);
  expect(files.length).toBe(original.length);
  files.forEach((f, i) => {
    // Every fragment insert lands between tags, and the file is whole XML
    // with the same text as the document it came from.
    const doc = parseXml(f);
    expect(textContent(doc.root).replace(/\s+/g, ' ').trim())
      .toBe(textContent(parseXml(original[i]).root).replace(/\s+/g, ' ').trim());
    expect(f).toStartWith('<?xml version="1.0" encoding="UTF-8"?>\n<html xmlns="http://www.w3.org/1999/xhtml">');
    expect(f).toContain('href="kindle:flow:0001?mime=text/css"');
  });
  for (const c of m.chunkTable) expect(c.length).toBeLessThanOrEqual(CHUNK_SIZE + 64);
  // Every aid's offset is the start of the tag that carries it.
  const rebuilt = new TextEncoder().encode(files.join(''));
  for (const [aid, [, , offset]] of m.aidOffsets) {
    const tag = dec.decode(rebuilt.slice(offset, offset + 400)).split('>')[0];
    expect(tag).toMatch(new RegExp(` [ac]id="${aid}"$`));
  }
  return { m, files };
}

describe('KF8 markup and the chunker', () => {
  test('a small file is one skeleton and one fragment, with aids on every text element', () => {
    const body = '<section class="scene" id="sc-001">\n<h2 class="scene-heading">INT. ROOM - DAY</h2>\n<p class="action">A &amp; B.</p>\n</section>\n';
    const { m, files } = checkMarkup(bookOf([body]), [serializeDoc(bookOf([body]).spine[0].doc)]);
    expect(m.skelTable).toEqual([{ fileNumber: 0, chunkCount: 1, startPos: 0, length: m.skelTable[0].length }]);
    expect(m.chunkTable[0].selector).toBe("P-//*[@aid='0']");
    expect(files[0]).toContain('<body aid="0">');
    expect(files[0]).toContain('<section class="scene" id="sc-001" aid="1">');
    expect(m.idMap.get(idKey('text/f0.xhtml', 'sc-001'))).toBe('1');
    expect(m.idMap.get(idKey('text/f0.xhtml', ''))).toBe('0');
  });

  test('a section larger than a fragment is stepped into, and big text is cut in AmznBigTextBlock spans', () => {
    const paras = Array.from({ length: 120 }, (_, i) => `<p class="action">Line ${i} of action that runs on for a while.</p>\n`).join('');
    const huge = `<p class="dialogue">${'Words, words, words. '.repeat(600)}</p>\n`;
    const body = `<section class="scene" id="sc-001">\n${paras}${huge}</section>\n<section class="scene" id="sc-002">\n<p>Short.</p>\n</section>\n`;
    const sources = [serializeDoc(bookOf([body]).spine[0].doc)];
    const { m, files } = checkMarkup(bookOf([body]), sources);
    expect(m.chunkTable.length).toBeGreaterThan(2);
    expect(m.chunkTable.some((c) => c.selector.startsWith('S-'))).toBe(true);
    expect(files[0]).toContain('<span class="AmznBigTextBlock">');
  });

  test('several files: aids are numbered per file, positions run on across them', () => {
    const bodies = ['<p id="a">One</p>\n', '<p id="b">Two</p>\n<p>Three</p>\n'];
    const sources = bodies.map((b) => serializeDoc(bookOf([b]).spine[0].doc));
    const { m } = checkMarkup(bookOf(bodies), sources);
    expect(m.idMap.get(idKey('text/f1.xhtml', 'b'))).toBe('UGI1');
    expect(m.skelTable[1].startPos).toBeGreaterThan(m.skelTable[0].startPos);
    expect(m.chunkTable[1].fileNumber).toBe(1);
  });

  test('tables get no aid, and an id inside one gets a cid', () => {
    const body = '<table class="dual-dialogue"><tr><td id="x"><p>L</p></td><td><p>R</p></td></tr></table>\n';
    const sources = [serializeDoc(bookOf([body]).spine[0].doc)];
    const { files, m } = checkMarkup(bookOf([body]), sources);
    expect(files[0]).toContain('<td id="x" cid="c1">');
    expect(files[0]).not.toMatch(/<table[^>]* aid=/);
    expect(m.idMap.get(idKey('text/f0.xhtml', 'x'))).toBe('c1');
  });

  test('pictures become kindle:embed references, and a link to another file a kindle:pos:fid link', () => {
    const book = bookOf(['<p><img src="../images/a.png" alt=""/></p>\n<p><a href="f1.xhtml#t">go</a></p>\n', '<p id="t">there</p>\n']);
    const m = kf8Markup(book, new Map([['images/a.png', { index: 1, mime: 'image/png' }]]));
    const files = rebuildFiles(m);
    expect(files[0]).toContain('src="kindle:embed:0001?mime=image/png"');
    expect([...m.usedImages]).toEqual(['images/a.png']);
    const link = /href="kindle:pos:fid:([0-9A-V]{4}):off:([0-9A-V]{10})"/.exec(files[0]);
    expect(link).not.toBeNull();
    const target = m.aidOffsets.get(m.idMap.get(idKey('text/f1.xhtml', 't'))!)!;
    expect(parseInt(link![1], 32)).toBe(target[0]);
    expect(parseInt(link![2], 32)).toBe(target[1]);
  });

  test('epub:type loses its prefix and the namespace declarations go, as Calibre writes them', () => {
    const body = '<section epub:type="titlepage" class="titlepage"><h1>T</h1></section>\n';
    const { files } = checkMarkup(bookOf([body]), [serializeDoc(bookOf([body]).spine[0].doc)]);
    expect(files[0]).toContain('<section type="titlepage" class="titlepage" aid="1">');
    expect(files[0]).not.toContain('xmlns:epub');
  });

  test('every committed fixture rebuilds whole', async () => {
    for (const name of ['field-station', 'images', 'torture'] as const) {
      const book = await readEpub(await fixtureEpub(name));
      const sources = book.spine.map((s) => serializeDoc(s.doc));
      checkMarkup(book, sources);
    }
  });
});

function serializeDoc(doc: { root: Parameters<typeof textContent>[0] }): string {
  // Only the text matters to the comparison, so any faithful serialization
  // will do; the reader's own is the simplest.
  return `<r>${escape(textContent(doc.root))}</r>`;
}
function escape(s: string) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;');
}
