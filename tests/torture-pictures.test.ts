// The pictures torture.pdf carries, so a device pass reads them in the same
// book as every other row (registry #19 for which are kept, #19a for how the
// book shows them). Each one is there for a question a desktop render cannot
// settle; tools/make-fixture.py states each one's geometry and its margin to
// the threshold that governs it, and tools/torture-manifest.json says what the
// device is asked about each.
//
// Same split by concern as torture.test.ts / torture-layout /
// torture-markup / torture-coverage: torture.test.ts parses the text alone and
// never sees a picture, so nothing here can move an assertion there.
import { beforeAll, describe, expect, test } from 'bun:test';
import JSZip from 'jszip';
import { extractDocument, type ExtractResult } from '../src/parser/extract';
import { parseLines } from '../src/parser/index';
import { convertPdf } from '../src/convert';
import type { ScreenplayElement } from '../src/parser/types';

/** Fresh bytes every time: getDocument TRANSFERS the buffer. */
const torture = () =>
  Bun.file('tests/fixtures/torture.pdf').arrayBuffer().then((b) => new Uint8Array(b));

// Sheet numbers here are PDF pages, as the picture filenames count them: the
// title sheet is page 1, so `p015-01.png` is the first picture on page 15.
const CARD = 'p001-01.png';
const PANEL = 'p015-01.png';
const PLATE = 'p016-01.png';
const TALL = 'p017-01.png';

describe('which pictures torture.pdf keeps (#19)', () => {
  let out: ExtractResult;
  beforeAll(async () => {
    out = await extractDocument(await torture());
  }, 60000);

  test('the title card, the panel, the plate and the tall plate, one sheet each', () => {
    // One sheet each is the point: repetition cannot reach any of them, so
    // every one has to arrive as art on its own merits.
    expect(out.pageCount).toBe(17);
    expect(out.images.map((i) => [i.pageNum, i.file])).toEqual([
      [1, CARD], [15, PANEL], [16, PLATE], [17, TALL],
    ]);
  });

  test('the letterhead on every sheet is refused as furniture, and nothing else is refused', () => {
    // The furniture rule at document scale: one logo on all 17 sheets, far
    // past the three-sheet threshold. images.pdf owns the other five rules.
    expect(out.imageStats.droppedBy).toEqual({
      pixels: 0, width: 0, height: 0, area: 0, furniture: 17, background: 0,
    });
    expect(out.imageStats.overCap).toBe(0);
    expect(out.imageStats.failed).toBe(0);
  });
});

describe('what each picture becomes in the element stream (#19)', () => {
  let elements: ScreenplayElement[];
  beforeAll(async () => {
    const out = await extractDocument(await torture());
    elements = parseLines(out.lines, out.images).elements;
  }, 60000);
  const at = (file: string) => elements.findIndex((el) => el.img?.file === file);

  test('each picture carries the width and the full-page verdict its sheet gives it', () => {
    // The card 522x702pt and the plate 558x666pt are both over 70% of their
    // sheet and wider than the text column; the panel is 180pt (42% of the
    // column) and the tall plate 240pt (56%), both under the full-page bar.
    expect(elements.filter((el) => el.type === 'image').map((el) => el.img)).toEqual([
      { file: CARD, widthPct: 100, fullPage: true },
      { file: PANEL, widthPct: 42, fullPage: false },
      { file: PLATE, widthPct: 100, fullPage: true },
      { file: TALL, widthPct: 56, fullPage: false },
    ]);
  });

  test('the title card comes before the title, so it stands ahead of every scene heading', () => {
    expect(at(CARD)).toBe(0);
    expect(elements[1].text).toBe('THE PROOF SHEET');
  });

  test('the panel sits between its heading and the action below it', () => {
    const i = at(PANEL);
    expect(elements[i - 1].type).toBe('scene');
    expect(elements[i - 1].text).toBe('INT. ART DEPARTMENT - DAY');
    expect(elements[i + 1].text).toContain('storyboard panel');
  });

  test('the full-page plate follows its scene heading with nothing between', () => {
    const i = at(PLATE);
    expect(elements[i - 1].type).toBe('scene');
    expect(elements[i - 1].text).toBe('INT. MAP ROOM - NIGHT');
  });

  test('the tall plate follows its heading and its one line of action', () => {
    const i = at(TALL);
    expect(elements[i - 2].text).toBe('INT. FIRE STAIRS - CONTINUOUS');
    expect(elements[i - 1].text).toBe('A production still, printed tall.');
  });
});

describe('the book built from torture.pdf (#19a)', () => {
  let zip: JSZip;
  let body = '';
  let nav = '';
  beforeAll(async () => {
    const result = await convertPdf(await torture());
    zip = await JSZip.loadAsync(result.epub);
    const files = Object.keys(zip.files)
      .filter((n) => /^OEBPS\/text\/body\d+\.xhtml$/.test(n))
      .sort();
    body = (await Promise.all(files.map((n) => zip.file(n)!.async('string')))).join('');
    nav = await zip.file('OEBPS/nav.xhtml')!.async('string');
  }, 60000);

  const figure = (file: string, cls: string) =>
    `<p class="${cls}"><img src="../images/${file}" alt="Illustration"/></p>`;

  test('carries exactly the four pictures, and no letterhead', () => {
    const images = Object.keys(zip.files)
      .filter((n) => n.startsWith('OEBPS/images/') && !zip.files[n].dir)
      .sort();
    expect(images).toEqual([CARD, PANEL, PLATE, TALL].map((f) => `OEBPS/images/${f}`));
  });

  test('the title card is an Opening section of its own, ahead of the first scene', () => {
    // No cover in this phase: a full-page picture before the first scene
    // heading opens an "Opening" section, and that section holds the card
    // and nothing else.
    expect(body).toContain(
      `<section class="scene" id="sc-001">\n${figure(CARD, 'figure w100 figure-full')}\n</section>`,
    );
    const toc = [...nav.matchAll(/<li><a href="[^"]*">([^<]*)<\/a><\/li>/g)].map((m) => m[1]);
    expect(toc[0]).toBe('Opening');
    expect(toc[1]).toBe('INT. ARCHIVE BASEMENT - NIGHT');
  });

  test('the panel and the tall plate stay in the flow at their own widths', () => {
    // 42 and 56 round to the nearest step of five, and neither is full-page:
    // the tall plate must not get forced breaks, or they would hide the
    // overflow the device is asked about.
    expect(body).toContain(figure(PANEL, 'figure w40'));
    expect(body).toContain(figure(TALL, 'figure w55'));
  });

  test('a scene heading sits directly above the full-page plate', () => {
    // From the closing tag on: the heading opens its sheet, so it may lead
    // with that sheet's page marker.
    expect(body).toContain(
      `INT. MAP ROOM - NIGHT</h2>\n${figure(PLATE, 'figure w100 figure-full')}`,
    );
  });
});
