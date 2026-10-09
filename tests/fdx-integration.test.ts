// The FDX end to end on the committed invented scripts: PDF → .fountain →
// FDX, checked as well-formed XML whose paragraph sequence is exactly the
// one the fountain-js tokens call for. The expected sequence is worked out
// here from the tokens with this file's OWN table, not the renderer's, so a
// renderer that drops or invents a paragraph cannot agree with itself.
import { beforeAll, describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { Fountain, type Token } from 'fountain-js';
import { convertPdf } from '../src/convert';
import { fountainToFdx } from '../src/fdx/fdx';
import { stripNotes } from '../src/fountain/notes';
import { PRIMARY_SLUG } from '../src/fountain/slug';
import { childElements, parseXml, textOf, type XmlElement } from './helpers/mini-xml';

const ROOT = new URL('..', import.meta.url).pathname;

const TYPE_FOR: Record<string, string> = {
  action: 'Action',
  centered: 'Action',
  character: 'Character',
  parenthetical: 'Parenthetical',
  dialogue: 'Dialogue',
  lyrics: 'Lyrics',
  transition: 'Transition',
};

/** The paragraph sequence the tokens call for: one entry per non-blank
 * line of each content token, a dual exchange as `General[` … `]`. */
function expectedSequence(tokens: Token[]): string[] {
  const out: string[] = [];
  for (const t of tokens) {
    if (t.is_title) continue;
    if (t.type === 'dual_dialogue_begin') { out.push('General['); continue; }
    if (t.type === 'dual_dialogue_end') { out.push(']'); continue; }
    const type = t.type === 'scene_heading'
      ? (PRIMARY_SLUG.test(t.text ?? '') ? 'Scene Heading' : 'Shot')
      : TYPE_FOR[t.type];
    if (!type) continue;
    const lines = stripNotes(t.text ?? '').split('\n').filter((l) => l.trim() !== '');
    for (let i = 0; i < lines.length; i++) out.push(type);
  }
  return out;
}

function actualSequence(content: XmlElement): string[] {
  const out: string[] = [];
  for (const p of childElements(content, 'Paragraph')) {
    const dual = childElements(p, 'DualDialogue')[0];
    if (dual) {
      out.push(`${p.attrs.Type}[`);
      for (const q of childElements(dual, 'Paragraph')) out.push(q.attrs.Type);
      out.push(']');
    } else {
      out.push(p.attrs.Type);
    }
  }
  return out;
}

function allParagraphs(el: XmlElement): XmlElement[] {
  const found: XmlElement[] = [];
  for (const c of childElements(el)) {
    if (c.name === 'Paragraph') found.push(c);
    found.push(...allParagraphs(c));
  }
  return found;
}

for (const name of ['field-station', 'screenplay', 'torture']) {
  describe(`${name}.pdf as FDX`, () => {
    let fountainText = '';
    let fdx = '';
    let root: XmlElement;

    beforeAll(async () => {
      const bytes = new Uint8Array(readFileSync(join(ROOT, 'tests', 'fixtures', `${name}.pdf`)));
      fountainText = (await convertPdf(bytes)).fountainText;
      fdx = fountainToFdx(fountainText);
      root = parseXml(fdx);
    });

    test('is well-formed XML with a FinalDraft root', () => {
      expect(root.name).toBe('FinalDraft');
      expect(root.attrs.DocumentType).toBe('Script');
    });

    test('its paragraph types follow the fountain-js token sequence exactly', () => {
      const { tokens } = new Fountain().parse(fountainText, true);
      const content = childElements(root, 'Content')[0];
      const expected = expectedSequence(tokens);
      expect(expected.length).toBeGreaterThan(20);
      expect(actualSequence(content)).toEqual(expected);
    });

    test('carries a title page with the script’s title on it', () => {
      const { tokens } = new Fountain().parse(fountainText, true);
      const title = tokens.find((t) => t.type === 'title')?.text;
      expect(title).toBeTruthy();
      const tp = childElements(childElements(root, 'TitlePage')[0], 'Content')[0];
      const texts = childElements(tp, 'Paragraph').map(textOf);
      // Emphasis markers become styles, so compare without them.
      expect(texts).toContain(title!.split('\n')[0].replace(/[*_]/g, ''));
    });

    test('no note, sigil or page marker leaks into any paragraph', () => {
      expect(fdx).not.toContain('[[');
      expect(fdx).not.toContain(']]');
      for (const p of allParagraphs(root)) {
        if (childElements(p, 'DualDialogue').length > 0) continue;
        const text = textOf(p);
        const type = p.attrs.Type;
        if (type === 'Character') expect(text).not.toMatch(/^@|\^\s*$/);
        if (type === 'Shot' || type === 'Scene Heading') expect(text).not.toMatch(/^\./);
        if (type === 'Transition') expect(text).not.toMatch(/^>/);
        if (type === 'Action') expect(text).not.toMatch(/^!|^= pg /);
      }
    });

    test('every paragraph holds at least one Text run and no stray text between runs', () => {
      for (const p of allParagraphs(root)) {
        if (childElements(p, 'DualDialogue').length > 0) {
          expect(p.children).toHaveLength(1);
          continue;
        }
        expect(childElements(p, 'Text').length).toBeGreaterThan(0);
        expect(p.children.every((c) => typeof c !== 'string')).toBe(true);
      }
    });
  });
}
