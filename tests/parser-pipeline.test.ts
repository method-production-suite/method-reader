import { describe, test, expect } from 'bun:test';
import { Fountain } from 'fountain-js';
import { extractDocument } from '../src/parser/extract';
import { parseLines } from '../src/parser/index';
import { toFountain } from '../src/fountain/serialize';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';
import type { RawImage, RawLine } from '../src/parser/types';

// The first direct tests of the post-extraction pipeline (parseLines):
// everything else exercises it only through fixture PDFs.

function line(text: string, indent: number, y: number, pageNum: number): RawLine {
  return { text, indent, y, pageNum };
}

/// A miniature script as extracted lines. The title page is the trap: a
/// centered uppercase title lands at ~38% indent, square inside the
/// character-cue band (35-50), and used to walk straight into the roster.
function miniature(): RawLine[] {
  return [
    // page 1 — title page, centered lines
    line('THE MOTH ARCHIVE', 38, 500, 1),
    line('written by', 44, 452, 1),
    line('A. N. Placeholder', 39, 428, 1),
    // page 2 — the script proper
    line('INT. READING ROOM - NIGHT', 17, 700, 2),
    line('The lamp flickers, thinks better of it, and holds.', 17, 676, 2),
    line('NOOR', 43, 652, 2),
    line('Box forty-one. No label, no date, no excuse.', 29, 640, 2),
    line('ELIAS', 43, 616, 2),
    line('Sign it or say why not.', 29, 604, 2),
    line('INT. STAIRWELL NINE - LATER', 17, 580, 2),
    line('Coffee in enamel cups. Nobody drinks; everybody holds.', 17, 556, 2),
    line('NOOR', 43, 532, 2),
    line('Lock the cage. Leave the light.', 29, 520, 2),
  ];
}

describe('parseLines metadata', () => {
  test('a title landing in the cue indent band stays out of the roster', () => {
    const sp = parseLines(miniature());
    const names = sp.characters.map((c) => c.name);
    expect([...names].sort()).toEqual(['ELIAS', 'NOOR']);
    expect(names).not.toContain('THE MOTH ARCHIVE');
  });

  test('dialogue counts drive the roster order', () => {
    const sp = parseLines(miniature());
    expect(sp.characters[0].name).toBe('NOOR');
    expect(sp.characters[0].dialogueCount).toBe(2);
  });

  test('scenes and pages come out of the same pass', () => {
    const sp = parseLines(miniature());
    expect(sp.scenes.length).toBe(2);
    expect(sp.pageCount).toBe(2);
  });
});

/// Classification (priority 9) runs BEFORE suppressBoilerplate, so anything
/// the mini-slug rule claims leaves the recurrence layer's view. A left-flush
/// all-caps watermark is mini-slug-shaped, and it recurs by definition — the
/// two passes have to be told about each other or every page grows a heading.
describe('mini-slugs and the recurrence suppressor', () => {
  /// `slugOnPages` pages carry `mark` at the action margin; every page also
  /// carries a heading and a line of action so the doc has a real page count.
  function pages(pageCount: number, mark: string, markOnPages: number[]): RawLine[] {
    const lines: RawLine[] = [];
    for (let p = 1; p <= pageCount; p++) {
      if (markOnPages.includes(p)) lines.push(line(mark, 17, 720, p));
      lines.push(line(`INT. ROOM ${p} - DAY`, 17, 660, p));
      lines.push(line(`Someone crosses the floor on page ${p}.`, 17, 600, p));
    }
    return lines;
  }
  const all = (n: number) => Array.from({ length: n }, (_, i) => i + 1);
  const typesOf = (ls: RawLine[], text: string) =>
    new Set(parseLines(ls).elements.filter((e) => e.text === text).map((e) => e.type));

  test('a per-page watermark does not escape into a heading on every page', () => {
    // 10 pages, threshold max(3, 40%) = 4. Before mini-slug was a recurrence
    // candidate this rendered ten bold micro-headings.
    expect(typesOf(pages(10, 'CONFIDENTIAL', all(10)), 'CONFIDENTIAL')).not.toContain('mini-slug');
  });

  test('a suppressed mini-slug is hidden on the same terms as action', () => {
    // page-number is the suppressor's sink and every consumer skips it. One
    // rule for both types, so a mark that types action on one page and
    // mini-slug on the next cannot come out hidden here and visible there.
    expect([...typesOf(pages(10, 'PROPERTY OF THE STUDIO', all(10)), 'PROPERTY OF THE STUDIO')])
      .toEqual(['page-number']);
  });

  test('a mini-slug that legitimately repeats under the threshold survives', () => {
    // 3 of 20 pages against a threshold of 8. Mini-slugs do repeat, so the
    // threshold is what has to hold — and it does: the widest page-spread of
    // any real mini-slug in the corpus is 14%, against a 40% bar.
    expect([...typesOf(pages(20, 'LATER', [4, 9, 15]), 'LATER')]).toEqual(['mini-slug']);
  });
});

describe('scene numbers vs page numbers', () => {
  // Both arrive as `page-number` elements, because a standalone shooting-script
  // scene number is deliberately typed that way so it stays hidden
  // (classify.ts, "Shooting script scene numbers"). attachSceneNumbers then
  // promotes it onto the following heading.
  //
  // The trap: a BARE page number like "1." is caught by PAGE_NUMBER_BARE at
  // priority 1 and is textually indistinguishable from a scene number, so any
  // page that OPENS with a scene heading used to donate its page number to
  // that scene. Ordinary scripts do that constantly.

  function pageOpeningOnHeading(numberText: string): RawLine[] {
    return [
      line(numberText, 87, 740, 2), // top-right gutter, first thing on page 2
      line('INT. ARCHIVE - NIGHT', 18, 700, 2),
      line('Someone is already here.', 18, 676, 2),
      line('BUNNY', 44, 640, 2),
      line('You came back.', 26, 628, 2),
    ];
  }

  test('a bare page number is NOT absorbed as a scene number', () => {
    const { elements } = parseLines(pageOpeningOnHeading('2.'));
    const scene = elements.find((e) => e.type === 'scene');
    expect(scene).toBeDefined();
    expect(scene!.sceneNumber).toBeUndefined();
  });

  test('a shooting-script scene number IS still attached', () => {
    // The documented path must keep working: this is why the rule exists.
    const { elements } = parseLines(pageOpeningOnHeading('1A.'));
    const scene = elements.find((e) => e.type === 'scene');
    expect(scene).toBeDefined();
    expect(scene!.sceneNumber).toBe('1A.');
  });

  test('a compound shooting-script number is still attached', () => {
    const { elements } = parseLines(pageOpeningOnHeading('2.2.'));
    const scene = elements.find((e) => e.type === 'scene');
    expect(scene!.sceneNumber).toBe('2.2.');
  });

  test('the page number is still stripped from the body either way', () => {
    const { elements } = parseLines(pageOpeningOnHeading('2.'));
    expect(elements.some((e) => e.type === 'page-number' && e.text === '2.')).toBe(true);
  });
});

describe('page furniture between a cue and its dialogue', () => {
  // A cue as the LAST line of a page, its speech resuming on the next page,
  // with the printed page number in between. Found on a real Kindle: speech
  // thirty-seven of the proof sheet rendered as "BUNNY Speech thirty-seven."
  // in one run instead of a cue and a speech.
  //
  // Page furniture is stripped from the output, so it must not influence
  // structure. Letting it reset the speaker breaks the speech in half.
  function cueStrandedAtPageEnd(): RawLine[] {
    return [
      line('BUNNY', 44, 72, 1),        // last line of page 1
      line('13.', 87, 756, 2),         // printed page number, top of page 2
      line('Speech thirty-seven. Short.', 29, 720, 2),
      line('CASSIUS', 44, 696, 2),
      line('And the reply.', 29, 684, 2),
    ];
  }

  test('dialogue after the page number still belongs to the cue', () => {
    const { elements } = parseLines(cueStrandedAtPageEnd());
    const speech = elements.find((e) => e.text.startsWith('Speech thirty-seven'));
    expect(speech).toBeDefined();
    expect(speech!.type).toBe('dialogue');
    expect(speech!.character).toBe('BUNNY');
  });

  test('the cue itself still classifies as a character', () => {
    const { elements } = parseLines(cueStrandedAtPageEnd());
    expect(elements.find((e) => e.text === 'BUNNY')?.type).toBe('character');
  });

  test('the page number is still stripped', () => {
    const { elements } = parseLines(cueStrandedAtPageEnd());
    expect(elements.find((e) => e.text === '13.')?.type).toBe('page-number');
  });

  test('furniture does not invent a speaker where there was none', () => {
    // Action after a page number must stay action: transparency must not
    // become "attach anything that follows furniture to the last speaker".
    const { elements } = parseLines([
      line('BUNNY', 44, 72, 1),
      line('Her line.', 29, 60, 1),
      line('13.', 87, 756, 2),
      line('She crosses to the window.', 18, 720, 2),
    ]);
    const act = elements.find((e) => e.text.startsWith('She crosses'));
    expect(act!.type).toBe('action');
  });
});

describe('a speech split by a page boundary is one speech', () => {
  // Device-confirmed on a Kindle: a speech spanning a page in the SOURCE pdf
  // stayed two elements, so the book rendered "...at the top or the" and
  // "bottom of a page..." as two paragraphs with a hard break mid-sentence.
  //
  // The earlier furniture fix gave both halves the right speaker. It never
  // merged their TEXT. Registry 8's rejoin only fires when the script prints
  // (MORE)/(CONT'D); celtx does not, and shows 9 of these seams.
  function speechAcrossAPage(): RawLine[] {
    return [
      line('ODILE', 44, 200, 1),
      line('Speech nineteen. If this line sits alone at the top or the', 29, 188, 1),
      line('13.', 87, 756, 2),
      line('bottom of a page, the keep failed.', 29, 720, 2),
    ];
  }

  test('the halves merge into a single dialogue element', () => {
    const { elements } = parseLines(speechAcrossAPage());
    const speeches = elements.filter((e) => e.type === 'dialogue');
    expect(speeches.length).toBe(1);
    expect(speeches[0].text).toBe(
      'Speech nineteen. If this line sits alone at the top or the bottom of a page, the keep failed.',
    );
  });

  test('the merged speech keeps its speaker', () => {
    const { elements } = parseLines(speechAcrossAPage());
    expect(elements.find((e) => e.type === 'dialogue')?.character).toBe('ODILE');
  });

  test('two DIFFERENT speakers across a page are NOT merged', () => {
    // The merge must key on the character, or a page break between two
    // people's lines would fuse them into one speech.
    const { elements } = parseLines([
      line('ODILE', 44, 200, 1),
      line('Her line.', 29, 188, 1),
      line('13.', 87, 756, 2),
      line('WREN', 44, 720, 2),
      line('His line.', 29, 708, 2),
    ]);
    const speeches = elements.filter((e) => e.type === 'dialogue');
    expect(speeches.length).toBe(2);
    expect(speeches.map((s) => s.character)).toEqual(['ODILE', 'WREN']);
  });

  test('two paragraphs of one speech on the SAME page stay separate', () => {
    // A speech may legitimately hold two paragraphs. They arrive as two
    // dialogue elements on one page, and merging them would destroy a break
    // the writer put there. Only a page turn splits a paragraph in half.
    const { elements } = parseLines([
      line('ODILE', 44, 300, 1),
      line('First paragraph of the speech.', 29, 288, 1),
      line('Second paragraph, deliberately separate.', 29, 250, 1),
    ]);
    expect(elements.filter((e) => e.type === 'dialogue').length).toBe(2);
  });

  test('the merge does not reach across an ACTION line', () => {
    // Only page furniture is transparent. Real content between two speeches
    // means they are genuinely separate.
    //
    // Note what the parser already does here, which this test pins rather
    // than changes: action RESETS the speaker, so the line after it is not
    // dialogue at all. The guard that matters is that the first speech does
    // not absorb anything past the action line.
    const { elements } = parseLines([
      line('ODILE', 44, 200, 1),
      line('First half.', 29, 188, 1),
      line('She crosses to the window.', 18, 176, 1),
      line('Second half.', 29, 164, 1),
    ]);
    const speeches = elements.filter((e) => e.type === 'dialogue');
    expect(speeches.length).toBe(1);
    expect(speeches[0].text).toBe('First half.');
  });
});

describe('extraction returns pictures beside the text', () => {
  // The pipeline's INPUT, not parseLines: the one place in this file a real
  // PDF is opened, because the claim is about what one page loop hands over.
  // tests/images-fixture.test.ts pins the pictures in detail; this pins that
  // the text-side fields kept their shape when the image fields joined them.
  test('the images fixture keeps the art and refuses the furniture', async () => {
    const bytes = new Uint8Array(await Bun.file('tests/fixtures/images.pdf').arrayBuffer());
    const result = await extractDocument(bytes);
    expect(result.lines.length).toBeGreaterThan(0);
    expect(result.pageCount).toBe(5);
    expect(typeof result.spacingRepairs).toBe('number');
    expect(result.images.map((i) => i.file)).toEqual(['p001-01.png', 'p002-01.png']);
    expect(result.images.map((i) => i.pageNum)).toEqual([1, 2]);
    expect(result.imageStats.droppedBy).toEqual({
      pixels: 1, width: 1, height: 0, area: 0, furniture: 5, background: 1,
    });
  });
});

describe('images merged into the element stream', () => {
  // parseLines takes two ordered streams and interleaves them (registry #19).
  // Everything here is about POSITION: which element a picture lands next to,
  // and what the metadata passes make of one.

  /// A `RawImage` as `toRawImages` would hand it over — already named, already
  /// measured against the page. `y` is the TOP edge in PDF user space, the same
  /// axis and direction RawLine uses, so a high y is high on the sheet.
  function image(y: number, pageNum: number, file: string, over: Partial<RawImage> = {}): RawImage {
    return {
      file,
      pageNum,
      y,
      // 50% of the page is 71% of a 6-inch column, and nowhere near full-page.
      pageWidthPct: 50,
      pageHeightPct: 20,
      bytes: new Uint8Array([1]),
      ...over,
    };
  }
  const typesOf = (ls: RawLine[], imgs: RawImage[]) =>
    parseLines(ls, imgs).elements.map((e) => e.type);
  const onlyImage = (ls: RawLine[], imgs: RawImage[]) =>
    parseLines(ls, imgs).elements.filter((e) => e.type === 'image');

  test('an image lands between the elements it sat between on the page', () => {
    expect(
      typesOf(
        [
          line('INT. WAREHOUSE - DAY', 15, 700, 1),
          line('The panel above shows the shot.', 15, 500, 1),
        ],
        [image(600, 1, 'p001-01.png')],
      ),
    ).toEqual(['scene', 'image', 'action']);
  });

  test('an image above everything on its page comes first', () => {
    expect(
      typesOf(
        [line('INT. A - DAY', 15, 700, 1), line('Someone waits.', 15, 600, 1)],
        [image(760, 1, 'p001-01.png')],
      ),
    ).toEqual(['image', 'scene', 'action']);
  });

  test('an image below everything on its page comes last', () => {
    expect(
      typesOf(
        [line('INT. A - DAY', 15, 700, 1), line('Someone waits.', 15, 600, 1)],
        [image(100, 1, 'p001-01.png')],
      ),
    ).toEqual(['scene', 'action', 'image']);
  });

  test('several images and several elements on one page interleave', () => {
    // The case a single insertion point could not get right: three images,
    // none of them adjacent to each other in the finished stream.
    expect(
      typesOf(
        [
          line('INT. A - DAY', 15, 700, 1),
          line('Someone waits.', 15, 600, 1),
          line('Someone leaves.', 15, 500, 1),
        ],
        [image(650, 1, 'p001-01.png'), image(550, 1, 'p001-02.png'), image(450, 1, 'p001-03.png')],
      ),
    ).toEqual(['scene', 'image', 'action', 'image', 'action', 'image']);
  });

  test('the filename rides through untouched, in the order it was handed over', () => {
    // parseLines does NOT number anything: `toRawImages` assigned these names
    // where it assigned the order, and a second counter here would be a second
    // thing to keep in step. This pins the pass-through, not the naming rule.
    const els = onlyImage(
      [line('INT. A - DAY', 15, 700, 1)],
      [image(600, 1, 'p001-01.png'), image(400, 1, 'p001-02.png')],
    );
    expect(els.map((e) => e.img?.file)).toEqual(['p001-01.png', 'p001-02.png']);
    // And it is the element's TEXT as well, which is what keeps the element
    // non-empty: toFountain drops anything that trims to nothing, and an image
    // has no words of its own to put there.
    expect(els.map((e) => e.text)).toEqual(['p001-01.png', 'p001-02.png']);
  });

  test('two images at the same y keep the order they arrived in', () => {
    // Equal y is not a tie the merge has to break: it drains the queue in
    // order, so the paint order `toRawImages`'s stable sort preserved survives
    // one more stage. A comparator-based insertion could quietly reverse them.
    expect(
      onlyImage(
        [line('INT. A - DAY', 15, 700, 1), line('Someone waits.', 15, 500, 1)],
        [image(600, 1, 'p001-01.png'), image(600, 1, 'p001-02.png')],
      ).map((e) => e.img?.file),
    ).toEqual(['p001-01.png', 'p001-02.png']);
  });

  test('the rendered width is remapped from the page to the text column', () => {
    // 50% of an 8.5" sheet is 71% of the 6.0" column the script is set in.
    expect(
      onlyImage([line('INT. A - DAY', 15, 700, 1)], [image(600, 1, 'p001-01.png')])[0].img!.widthPct,
    ).toBe(71);
  });

  test('a picture wider than the column is asked to fill the column, no more', () => {
    // The title-card case: 85% of the sheet is 121% of the column.
    const [img] = onlyImage(
      [line('INT. A - DAY', 15, 700, 1)],
      [image(600, 1, 'p001-01.png', { pageWidthPct: 85 })],
    );
    expect(img.img!.widthPct).toBe(100);
  });

  test('a full-page image is flagged, an inline one is not', () => {
    // Both halves, or the flag could be a constant `true`. 90 x 88 is 79% of
    // the page against the 70% bar; the helper's 50 x 20 is 10%.
    const big = onlyImage(
      [line('INT. A - DAY', 15, 700, 1)],
      [image(750, 1, 'p001-01.png', { pageWidthPct: 90, pageHeightPct: 88 })],
    );
    expect(big[0].img!.fullPage).toBe(true);
    expect(
      onlyImage([line('INT. A - DAY', 15, 700, 1)], [image(600, 1, 'p001-01.png')])[0].img!.fullPage,
    ).toBe(false);
  });

  test('an images array in the wrong order is normalized, not misplaced', () => {
    // parseLines is exported, so any caller can hand it pictures. Today
    // `toRawImages` hands this half over already sorted, so the sort inside
    // insertImages is a no-op — which is exactly why it needs its own test: a
    // caller that builds RawImage[] some other way must not silently scatter
    // pictures through the book. Handed over backwards on purpose.
    const sp = parseLines(
      [
        line('INT. A - DAY', 15, 700, 1),
        line('Someone waits.', 15, 500, 1),
        line('INT. B - DAY', 15, 700, 2),
      ],
      [image(600, 2, 'p002-01.png'), image(600, 1, 'p001-01.png')],
    );
    expect(sp.elements.map((e) => e.type)).toEqual([
      'scene', 'image', 'action', 'scene', 'image',
    ]);
    expect(sp.elements.filter((e) => e.type === 'image').map((e) => e.img!.file))
      .toEqual(['p001-01.png', 'p002-01.png']);
  });

  test('an image level with a line sits below it: the two y are different edges', () => {
    // A line's y is its BASELINE (extract.ts takes the text item's own origin)
    // and an image's is its TOP edge, so a picture whose y equals a line's y
    // starts where that line's glyphs sit and runs downward. Equality is
    // therefore not a coin toss: the line is above, and goes first.
    expect(
      typesOf(
        [line('INT. A - DAY', 15, 700, 1), line('Someone waits.', 15, 600, 1)],
        [image(600, 1, 'p001-01.png')],
      ),
    ).toEqual(['scene', 'action', 'image']);
  });

  test('ordering is by page first and only then down the page', () => {
    // The image on page 1 sits at the BOTTOM of it (y 100) and the one on
    // page 2 at the top (y 750): compare y alone and they come out reversed.
    expect(
      typesOf(
        [line('INT. A - DAY', 15, 700, 1), line('INT. B - DAY', 15, 700, 2)],
        [image(100, 1, 'p001-01.png'), image(750, 2, 'p002-01.png')],
      ),
    ).toEqual(['scene', 'image', 'image', 'scene']);
  });

  test('an image on a page with no text at all still lands between the pages', () => {
    // A plate sheet: nothing on page 2 but the picture.
    expect(
      typesOf(
        [line('INT. A - DAY', 15, 700, 1), line('INT. C - DAY', 15, 700, 3)],
        [image(400, 2, 'p002-01.png')],
      ),
    ).toEqual(['scene', 'image', 'scene']);
  });

  test('an image past the last text element is kept, and counts toward pageCount', () => {
    // A closing full-page plate. The tail loop is the only thing that carries
    // it, and the sheet it sits on is a real page of the document.
    const sp = parseLines([line('INT. A - DAY', 15, 700, 1)], [image(700, 4, 'p004-01.png')]);
    expect(sp.elements.map((e) => e.type)).toEqual(['scene', 'image']);
    expect(sp.pageCount).toBe(4);
  });

  test('an image is never a speaking character and never a scene', () => {
    const sp = parseLines([line('INT. A - DAY', 15, 700, 1)], [image(600, 1, 'p001-01.png')]);
    expect(sp.scenes.length).toBe(1);
    expect(sp.characters.length).toBe(0);
  });

  test('scene elementIndex counts the images that precede it', () => {
    // Why insertImages runs BEFORE the metadata passes: elementIndex is an
    // index into the array the caller receives, so it has to be computed over
    // the finished stream.
    const sp = parseLines(
      [line('INT. A - DAY', 15, 700, 1), line('INT. B - DAY', 15, 400, 1)],
      [image(760, 1, 'p001-01.png'), image(500, 1, 'p001-02.png')],
    );
    expect(sp.elements.map((e) => e.type)).toEqual(['image', 'scene', 'image', 'scene']);
    expect(sp.scenes.map((s) => s.elementIndex)).toEqual([1, 3]);
  });

  test('character firstAppearance counts the images too', () => {
    // The same index into the same array as `elementIndex`, and it has no
    // consumer today — which is exactly why it is pinned here. A mutation
    // sweep showed that counting characters over the pre-image stream survives
    // everything else in the suite, so the day something reads this field it
    // would have read a stale index with every test green.
    const sp = parseLines(
      [line('INT. A - DAY', 15, 700, 1), line('NOOR', 43, 600, 1), line('Her line.', 29, 588, 1)],
      [image(760, 1, 'p001-01.png')],
    );
    expect(sp.elements.map((e) => e.type)).toEqual(['image', 'scene', 'character', 'dialogue']);
    expect(sp.characters).toEqual([
      { name: 'NOOR', baseName: 'NOOR', dialogueCount: 1, firstAppearance: 2 },
    ]);
  });

  test('omitting images changes nothing', () => {
    const lines = [line('INT. A - DAY', 15, 700, 1), line('Some action.', 15, 600, 1)];
    expect(parseLines(lines).elements.map((e) => e.type)).toEqual(['scene', 'action']);
    expect(parseLines(lines, []).elements).toEqual(parseLines(lines).elements);
  });

  test('a text element carries the y of its block, which is what the merge reads', () => {
    // Named directly because everything above rests on it: drop `y` in
    // classifyBlock and no image can find an anchor.
    const els = parseLines([
      line('INT. A - DAY', 15, 700, 1),
      line('Someone waits.', 15, 600, 1),
    ]).elements;
    expect(els.map((e) => e.y)).toEqual([700, 600]);
  });

  test('a speech split across a page does not derail the images after it', () => {
    // mergeSplitSpeeches folds the second half into the FIRST element, which
    // keeps page 1's y=188 while sitting between two page-2 elements. The
    // merge must not read that stale anchor as "page 2 is over".
    const els = parseLines(
      [
        line('ODILE', 44, 200, 1),
        line('Speech nineteen. If this line sits alone at the top or the', 29, 188, 1),
        line('13.', 87, 756, 2),
        line('bottom of a page, the keep failed.', 29, 720, 2),
        line('INT. NEXT ROOM - DAY', 17, 690, 2),
      ],
      [image(700, 2, 'p002-01.png')],
    ).elements;
    expect(els.map((e) => e.type)).toEqual([
      'character', 'dialogue', 'page-number', 'image', 'scene',
    ]);
    expect(els.filter((e) => e.type === 'dialogue').length).toBe(1);
  });

  test('a plate between the halves of a page-split speech does not split it', () => {
    // Why insertImages runs AFTER mergeSplitSpeeches. That pass joins the
    // halves only when nothing but furniture sits between them, and a plate
    // at the top of page 2, under the page number and above the continuation,
    // sits exactly there. Merged in first, the picture would end the run and
    // one speech would come out as two with the plate between them; merged in
    // after, the continuation is already folded into the first half.
    const els = parseLines(
      [
        line('ODILE', 44, 200, 1),
        line('Speech nineteen. If this line sits alone at the top or the', 29, 188, 1),
        line('13.', 87, 756, 2),
        line('bottom of a page, the keep failed.', 29, 720, 2),
        line('INT. NEXT ROOM - DAY', 17, 690, 2),
      ],
      [image(740, 2, 'p002-01.png')],
    ).elements;
    expect(els.map((e) => e.type)).toEqual([
      'character', 'dialogue', 'page-number', 'image', 'scene',
    ]);
    expect(els.filter((e) => e.type === 'dialogue').map((e) => e.text)).toEqual([
      'Speech nineteen. If this line sits alone at the top or the bottom of a page, the keep failed.',
    ]);
  });

  describe('a plate in a page seam, with page markers on', () => {
    // Each of these pins a serializer rule, and they are asserted HERE rather
    // than only in serialize.test.ts because their reachability is the whole
    // point: hand-built elements can be dismissed as a shape the parser never
    // makes, and these are shapes it makes from ordinary lines. Page 1 opens
    // on a heading so nothing on it is read as a title page.
    const markers = { ...DEFAULT_FORMAT_OPTIONS, showPageMarkers: true };

    test('a cue at the foot of a page keeps the speech under the next page plate', () => {
      const sp = parseLines(
        [
          line('INT. ROOM - DAY', 17, 700, 1),
          line('The lamp holds.', 17, 400, 1),
          line('BUNNY', 40, 72, 1),        // last line of page 1
          line('Her line.', 30, 700, 2),   // her speech, under the plate
          line('INT. NEXT - DAY', 17, 660, 2),
        ],
        [image(756, 2, 'p002-01.png')],    // the plate, above her speech
      );
      expect(sp.elements.map((e) => e.type)).toEqual([
        'scene', 'action', 'character', 'image', 'dialogue', 'scene',
      ]);
      expect(toFountain(sp, undefined, markers)).toBe(
        'INT. ROOM - DAY\n\nThe lamp holds.\n\n@BUNNY\nHer line.\n\n' +
          '[[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n',
      );
    });

    // A speech split by a page, with the plate somewhere in the seam. The two
    // cases below differ ONLY in the picture's y — below the continuation cue
    // or above it — and they must serialize the same, because where a plate
    // sits on the sheet is not a fact about whether a speech continues.
    const contdSeam = [
      line('INT. ROOM - DAY', 17, 700, 1),
      line('BUNNY', 40, 200, 1),
      line('First half.', 30, 188, 1),
      line("BUNNY (CONT'D)", 40, 740, 2),
      line('second half.', 30, 660, 2),
      line('INT. NEXT - DAY', 17, 600, 2),
    ];
    const seamOutput =
      'INT. ROOM - DAY\n\n@BUNNY\nFirst half.\nsecond half.\n\n' +
      '[[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n';

    test('a plate BELOW the cue does not re-split the rejoined speech', () => {
      const sp = parseLines(contdSeam, [image(700, 2, 'p002-01.png')]);
      // rejoinSplitDialogue drops the repeated cue, so the block is OPEN when
      // the plate arrives — which is the condition that makes this bite.
      expect(sp.elements.map((e) => e.type)).toEqual([
        'scene', 'character', 'dialogue', 'character', 'image', 'dialogue', 'scene',
      ]);
      expect(toFountain(sp, undefined, markers)).toBe(seamOutput);
    });

    test('a plate ABOVE the cue does not switch the rejoin off', () => {
      // A plate at the top of a sheet with the continuation cue under it is
      // ordinary layout, and it sits in the slot that prepare's rejoin loop
      // would read as "the previous element" if the loop did not look past
      // images. Read that way, the repeated cue survives and the reader gets a
      // duplicated (CONT'D) with one speech in two blocks. Same lines, same
      // picture, 60pt higher.
      const sp = parseLines(contdSeam, [image(760, 2, 'p002-01.png')]);
      expect(sp.elements.map((e) => e.type)).toEqual([
        'scene', 'character', 'dialogue', 'image', 'character', 'dialogue', 'scene',
      ]);
      expect(toFountain(sp, undefined, markers)).toBe(seamOutput);
    });
  });

  describe('THE EXCEPTION: an image inside a dual-dialogue region', () => {
    // The one place the element stream is NOT ordered by descending y.
    // deinterleaveDualDialogue emits the left column and then the right, and
    // the right column's y values restart at the top of the exchange — so the
    // first element below the picture is a left-column one, and that is where
    // it goes, in front of that column's first speech instead of after the
    // whole exchange.
    //
    // Pinned rather than fixed HERE: a picture printed inside a two-column
    // simultaneous speech is not a document the corpus contains, and the cost
    // of the misplacement is bounded — serialize.ts's cue lookahead sees
    // past an image and its emitter holds the note until the speech closes, so
    // a misplaced picture stays a misplaced picture and does not take a speech
    // with it. If these go red because someone gave both streams a second
    // ordering key, delete the tests rather than the fix.
    // As extraction hands it over: left column, then right, with the right
    // column's cue flagged `dualRight` (extract.ts sets it on that line only).
    const dualExchange = [
      line('BUNNY', 40, 500, 1),
      line('Left one.', 30, 488, 1),
      { ...line('CASSIUS', 40, 500, 1), dualRight: true },
      line('Right one.', 30, 488, 1),
    ];

    test('lands among the left column instead of after the exchange', () => {
      expect(typesOf(dualExchange, [image(494, 1, 'p001-01.png')])).toEqual([
        'character', 'image', 'dialogue', 'character', 'dialogue',
      ]);
    });

    test('and the cost is a misplaced picture, nothing more', () => {
      // The assertion the element types cannot make, and the reason this is
      // asserted one layer down. `prepare` in serialize.ts demotes a
      // `character` whose next element is neither dialogue nor parenthetical,
      // and the image is that next element — so a lookahead that stopped at
      // it would cost BUNNY her `@`, leaving one dialogue block to render as
      // three action paragraphs. The lookahead skips images and the note
      // waits for the speech to close: both speeches survive intact, and the
      // picture comes out one speech later than the sheet drew it.
      expect(toFountain(parseLines(dualExchange))).toBe(
        '@BUNNY\nLeft one.\n\n@CASSIUS ^\nRight one.\n',
      );
      const out = toFountain(parseLines(dualExchange, [image(494, 1, 'p001-01.png')]));
      expect(out).toBe(
        '@BUNNY\nLeft one.\n\n[[img: p001-01.png w=71]]\n\n@CASSIUS ^\nRight one.\n',
      );
      // Nor does the note between the columns break the pair: fountain-js
      // still reads one dual exchange, with the note inside it.
      expect(new Fountain().parse(out, true).tokens.map((t) => t.type)).toEqual([
        'dual_dialogue_begin',
        'dialogue_begin', 'character', 'dialogue', 'dialogue_end',
        'note',
        'dialogue_begin', 'character', 'dialogue', 'dialogue_end',
        'dual_dialogue_end',
      ]);
    });

    test('an image between a cue and its speech costs NOTHING in ONE column', () => {
      // The general case, and the one that is not an ordering exception at
      // all: nothing about this stream is out of order, a picture simply sat
      // between a cue and the line under it. What keeps the `@` and the
      // speaker here is serialize.ts's lookahead past the image, and this is
      // the test that goes red from the parser's side if it stops looking.
      const singleColumn = [
        line('BUNNY', 40, 500, 1),
        line('Her line.', 30, 470, 1),
      ];
      expect(toFountain(parseLines(singleColumn))).toBe('@BUNNY\nHer line.\n');
      expect(toFountain(parseLines(singleColumn, [image(480, 1, 'p001-01.png')]))).toBe(
        '@BUNNY\nHer line.\n\n[[img: p001-01.png w=71]]\n',
      );
    });
  });
});
