// Which picture is the book's cover (src/fountain/cover.ts). The renderers'
// halves, the EPUB declaring it and the flow leaving it out, are tested
// beside each renderer; this file is the rule alone.
import { describe, expect, test } from 'bun:test';
import { Fountain } from 'fountain-js';
import { pickCover } from '../src/fountain/cover';
import { DEFAULT_FORMAT_OPTIONS, resolveFormatOptions } from '../src/options';

const ON = DEFAULT_FORMAT_OPTIONS;
const HAVE = new Set(['p001-01.png', 'p002-01.png']);
const CARD = '[[img: p001-01.png w=100 full]]\n\n';
const SCRIPT = 'INT. LAB - DAY\n\nWork.\n\nEXT. YARD - NIGHT\n\nRest.\n';
const tokensOf = (src: string) => new Fountain().parse(src, true).tokens;
const coverOf = (src: string, format = ON, have: ReadonlySet<string> = HAVE) =>
  pickCover(tokensOf(src), format, have)?.file;

describe('pickCover', () => {
  test('a full-page picture before the first heading is the cover', () => {
    const tokens = tokensOf(CARD + SCRIPT);
    const cover = pickCover(tokens, ON, HAVE);
    expect(cover?.file).toBe('p001-01.png');
    // The note itself, so a renderer can leave out THIS note rather than
    // every note that names the same file.
    expect(cover?.token).toBe(tokens.find((t) => t.type === 'note'));
  });

  test('both options are on by default, so a card is a cover unless someone says otherwise', () => {
    expect(ON.includeImages).toBe(true);
    expect(ON.promoteCoverImage).toBe(true);
  });

  test('the title page block is not body content, so a card after it is still the cover', () => {
    expect(coverOf(`Title: T\nCredit: Written by\nAuthor: A\n\n${CARD}${SCRIPT}`)).toBe('p001-01.png');
  });

  test('a note that renders nothing does not stand in front of the card', () => {
    // A hand-written note, a picture whose file is missing, and one whose
    // name the allowlist refuses: none of them draws anything, so the card
    // is still the first thing the book would show.
    for (const before of ['[[a note to self]]', '[[img: p009-01.png w=50]]', '[[img: ../x.png w=100 full]]']) {
      expect(coverOf(`${before}\n\n${CARD}${SCRIPT}`), before).toBe('p001-01.png');
    }
  });

  test('a full-page plate after a heading is not the cover', () => {
    expect(coverOf(`INT. LAB - DAY\n\n${CARD}Work.\n`)).toBeUndefined();
  });

  test('a picture that is not full-page is not the cover, and the first picture decides', () => {
    const panel = '[[img: p002-01.png w=42]]\n\n';
    expect(coverOf(panel + SCRIPT)).toBeUndefined();
    // A full card after a smaller picture is not the book's face: the reader
    // would open on the smaller one.
    expect(coverOf(panel + CARD + SCRIPT)).toBeUndefined();
  });

  test('anything else before the card means no cover', () => {
    // Text a reader would see, and the tokens that draw nothing but open the
    // Opening section all the same. Our own writer puts none of these ahead
    // of a title card; a hand-written file that does keeps the card in the
    // flow, where it was in the book before covers.
    for (const before of [
      'A cold open.',
      '> FADE IN:',
      '> THE END <',
      '.MONTAGE',
      '= pg 1',
      '= A synopsis.',
      '# ACT ONE',
      '===',
    ]) {
      expect(coverOf(`${before}\n\n${CARD}${SCRIPT}`), before).toBeUndefined();
    }
  });

  test('a picture the book cannot ship is not the cover', () => {
    // rendersImage's three gates, each alone.
    expect(coverOf(CARD + SCRIPT, resolveFormatOptions({ includeImages: false }))).toBeUndefined();
    expect(coverOf(CARD + SCRIPT, ON, new Set(['p002-01.png']))).toBeUndefined();
    expect(coverOf(CARD + SCRIPT, ON, new Set())).toBeUndefined();
    const hostile = '[[img: ../../etc/p001-01.png w=100 full]]\n\n';
    expect(coverOf(hostile + SCRIPT, ON, new Set(['../../etc/p001-01.png']))).toBeUndefined();
  });

  test('promoteCoverImage off means no cover, the card is left where it is', () => {
    expect(coverOf(CARD + SCRIPT, resolveFormatOptions({ promoteCoverImage: false }))).toBeUndefined();
  });

  test('a script with no heading at all can still have a cover', () => {
    // Before the first heading is everywhere, in a document that has none.
    expect(coverOf(CARD + 'Just action.\n')).toBe('p001-01.png');
  });
});
