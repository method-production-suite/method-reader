import { describe, expect, test } from 'bun:test';
import JSZip from 'jszip';
import { convertFountain } from '../src/convert';
import { descendants, findAll, getAttr, parseXml, serialize, textContent } from '../src/azw3/xml';

describe('the AZW3 writer reads XML', () => {
  test('elements, attributes, text and entities', () => {
    const { root } = parseXml(
      '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE html>\n<html xmlns="x"><body>'
        + '<p class="a" id=\'b\'>Tom &amp; Jerry&apos;s &#x2014;&#8212; &lt;end&gt;</p><br/>tail</body></html>',
    );
    expect(root.name).toBe('html');
    const p = findAll(root, 'p')[0];
    expect(getAttr(p, 'class')).toBe('a');
    expect(getAttr(p, 'id')).toBe('b');
    expect(textContent(p)).toBe("Tom & Jerry's —— <end>");
    expect(textContent(root)).toEndWith('tail');
  });

  test('comments and processing instructions are dropped, and the text around them joins', () => {
    const { root } = parseXml('<a>one<!-- gone -->two<?pi x?>three</a>');
    expect(root.children).toEqual([{ kind: 'text', text: 'onetwothree' }]);
  });

  test('anything malformed throws rather than guessing', () => {
    expect(() => parseXml('<a><b></a>')).toThrow();
    expect(() => parseXml('<a>')).toThrow();
    expect(() => parseXml('<a>&nbsp;</a>')).toThrow(/entity/);
    expect(() => parseXml('<a x=1/>')).toThrow();
    expect(() => parseXml('<a/><b/>')).toThrow(/second root/);
  });

  test('a subtree serializes to exactly the bytes it occupies in the whole', () => {
    const { root } = parseXml('<r><p a="1&amp;2&quot;">x &lt; y<i/></p>\n<q>z</q></r>');
    const whole = serialize(root);
    for (const d of descendants(root)) expect(whole).toContain(serialize(d));
    expect(whole).toBe('<r><p a="1&amp;2&quot;">x &lt; y<i/></p>\n<q>z</q></r>');
  });

  test('closeEmpty writes text elements open and shut', () => {
    const { root } = parseXml('<r><p/><img/></r>');
    expect(serialize(root, { closeEmpty: new Set(['p']) })).toBe('<r><p></p><img/></r>');
  });

  test('reads every document our own EPUB writes, and writes the same text back', async () => {
    const { epub } = await convertFountain('Title: Test\n\nINT. ROOM - DAY\n\nA hall & a door.\n\nJO\nIt\'s <here>.\n');
    const zip = await JSZip.loadAsync(epub);
    const docs = Object.keys(zip.files).filter((n) => /\.(xhtml|opf|xml)$/.test(n));
    expect(docs.length).toBeGreaterThanOrEqual(4);
    for (const name of docs) {
      const xml = await zip.file(name)!.async('string');
      const once = serialize(parseXml(xml).root);
      expect(serialize(parseXml(once).root)).toBe(once);
      expect(textContent(parseXml(once).root)).toBe(textContent(parseXml(xml).root));
    }
  });
});
