import { describe, expect, test } from 'bun:test';
import { readEpub, resolveHref } from '../src/azw3/epub';
import { findAll, getAttr } from '../src/azw3/xml';
import { fixtureEpub } from './helpers/fixture-epubs';

describe('the AZW3 writer reads our EPUB back', () => {
  test('hrefs resolve against their folder', () => {
    expect(resolveHref('text', '../images/p002-01.png')).toBe('images/p002-01.png');
    expect(resolveHref('', 'text/body001.xhtml#sc-001')).toBe('text/body001.xhtml#sc-001');
    expect(resolveHref('OEBPS/text', './a%20b.xhtml')).toBe('OEBPS/text/a b.xhtml');
  });

  test('field-station: metadata, the spine in order, the stylesheet and the scene TOC', async () => {
    const book = await readEpub(await fixtureEpub('field-station'));
    expect(book.title.length).toBeGreaterThan(0);
    expect(book.authors.length).toBe(1);
    expect(book.language).toBe('en');
    expect(book.identifier).toStartWith('urn:');
    expect(book.spine[0].href).toBe('titlepage.xhtml');
    expect(book.spine.slice(1).every((s) => s.href.startsWith('text/body'))).toBe(true);
    expect([...book.stylesheets.keys()]).toEqual(['style.css']);
    expect(book.toc.length).toBe(11);
    expect(book.toc.every((t) => /^text\/body\d+\.xhtml#sc-\d+$/.test(t.href) && t.depth === 0)).toBe(true);
    // Every TOC target is an id in the spine document it names.
    for (const t of book.toc) {
      const [file, id] = t.href.split('#');
      const doc = book.spine.find((s) => s.href === file)!.doc;
      expect(findAll(doc.root, 'section').some((s) => getAttr(s, 'id') === id)).toBe(true);
    }
    expect(book.landmarks.map((l) => l.type)).toEqual(['titlepage', 'bodymatter']);
    expect(book.images).toEqual([]);
    expect(book.coverHref).toBeUndefined();
  });

  test('images: both pictures, in manifest order, and the cover named', async () => {
    const book = await readEpub(await fixtureEpub('images'));
    expect(book.images.map((i) => i.href)).toEqual(['images/p001-01.png', 'images/p002-01.png']);
    expect(book.images.every((i) => i.mediaType === 'image/png' && i.bytes[1] === 0x50)).toBe(true);
    expect(book.coverHref).toBe('images/p001-01.png');
  });
});
