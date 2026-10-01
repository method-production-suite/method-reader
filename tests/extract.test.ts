import '../src/parser/pdfjs-shims';
import { describe, test, expect } from 'bun:test';
import { existsSync } from 'node:fs';
import {
  collectUnderlineMarks,
  extractDocument,
  familyBucket,
  groupItemsIntoLines,
  markUnderlinesItem,
  repairPhantomSpaces,
  stampLineFmt,
} from '../src/parser/extract';
import type { FontRun, RawLine } from '../src/parser/types';
import { OPS } from 'pdfjs-dist/build/pdf.mjs';

function item(str: string, x: number, y: number) {
  return { str, transform: [1, 0, 0, 1, x, y] };
}

describe('groupItemsIntoLines', () => {
  test('items on one Y join into a single line with indent %', () => {
    const lines = groupItemsIntoLines([item('Hello', 61, 700), item('world', 120, 700)], 612, 1);
    expect(lines).toHaveLength(1);
    expect(lines[0].text).toBe('Hello world');
    expect(lines[0].indent).toBe(10); // 61/612
  });

  test("continuation suffix printed 1pt above its cue joins the cue line", () => {
    // Night Shift's generator prints "(CONT'D)" as a separate item one
    // point higher than the character name; exact-Y bucketing split it into
    // its own line, which then serialized as a stray "(CONT'D)" paragraph.
    const lines = groupItemsIntoLines(
      [item("(CONT'D)", 290, 268), item('MARGO', 250, 267), item('You look totally wiped.', 180, 255)],
      612,
      1,
    );
    expect(lines).toHaveLength(2);
    expect(lines[0].text).toBe("MARGO (CONT'D)");
    expect(lines[1].text).toBe('You look totally wiped.');
  });

  test('double-printed text at the same position is deduplicated', () => {
    // Final Draft double-prints (MORE) and (CONT'D) cue lines at identical
    // coordinates; naive joining yields "(MORE)(MORE)".
    const lines = groupItemsIntoLines(
      [item('(MORE)', 300, 80), item('(MORE)', 300, 80)],
      612,
      1,
    );
    expect(lines[0].text).toBe('(MORE)');
  });

  test('duplicates separated by an empty zero-width item still deduplicate', () => {
    // Final Draft emits "(MORE)", "", "(MORE)" — the empty item must not
    // break duplicate adjacency.
    const lines = groupItemsIntoLines(
      [item('(MORE)', 252, 63), item('', 252, 63), item('(MORE)', 252, 63)],
      612,
      1,
    );
    expect(lines[0].text).toBe('(MORE)');
  });

  test('right-margin revision stars on their own line are dropped', () => {
    // Revised drafts mark changed lines with * in the right margin; a bare
    // star line must not become an element (it would reset dialogue context).
    const lines = groupItemsIntoLines(
      [item('Hey! Careful with those tapes,', 180, 700), item('*', 575, 688)],
      612,
      1,
    );
    expect(lines).toHaveLength(1);
    expect(lines[0].text).toBe('Hey! Careful with those tapes,');
  });

  test('right-margin revision star on the same line as text is dropped', () => {
    const lines = groupItemsIntoLines(
      [item('He walks out.', 110, 700), item('*', 575, 700)],
      612,
      1,
    );
    expect(lines[0].text).toBe('He walks out.');
  });

  test('asterisk emphasis inside body text is kept', () => {
    const lines = groupItemsIntoLines([item('***THIS SCENE IS FILMED***', 110, 700)], 612, 1);
    expect(lines[0].text).toBe('***THIS SCENE IS FILMED***');
  });

  test('dual dialogue de-interleaves into left speech then right speech', () => {
    // p48 of The Last Video Store: MARGO ‖ DEV speaking simultaneously
    // in two columns. Naive Y-joining interleaves them into garbage.
    const lines = groupItemsIntoLines(
      [
        item('MARGO', 180, 700), item('DEV', 400, 700),
        item('rewind it again.', 150, 688), item('maybe if you were faster', 380, 688),
        item('at closing.', 380, 676), // right column runs one line longer
        item('They stare at the screen.', 108, 640), // back to normal action
      ],
      612,
      1,
    );
    expect(lines.map((l) => l.text)).toEqual([
      'MARGO',
      'rewind it again.',
      'DEV',
      'maybe if you were faster',
      'at closing.',
      'They stare at the screen.',
    ]);
    expect(lines[0].indent).toBe(40); // cue zone
    expect(lines[1].indent).toBe(30); // dialogue zone
    expect(lines[2].indent).toBe(40);
    expect(lines[4].indent).toBe(30);
  });

  test('a ONE-LETTER character name still anchors a dual region', () => {
    // Found in the wild 2026-09-16: a Final Draft script whose lead is named
    // "Q". isCueShaped required two characters AND two letters, so a
    // "ALANI  Q" cue row failed the dual test, the region was never entered,
    // and the two columns were Y-joined into one line each. The visible
    // damage was not a missing cue: it was every following speech fusing
    // with the other speaker's, and the rest of the scene reading as action,
    // because the interleaved lines carry no cue to attach to.
    //
    // The geometry is the real guard here — left of 42%, right of 48%, a
    // clear gap — so admitting a single letter costs no safety. A page
    // number cannot reach this: it has no letters at all.
    const lines = groupItemsIntoLines(
      [
        item('ALANI', 194, 700), item('Q', 432, 700),
        item('Hang on--', 150, 688), item('We talked about this.', 400, 688),
        item('She turns away.', 108, 640),
      ],
      612,
      1,
    );
    expect(lines.map((l) => l.text)).toEqual([
      'ALANI',
      'Hang on--',
      'Q',
      'We talked about this.',
      'She turns away.',
    ]);
  });

  test('a lone letter does NOT make any two-cluster row a dual cue', () => {
    // The pair of digits a shooting script prints in both margins, and a
    // scene number row, must not be read as two cues now that one letter is
    // allowed. Neither has a letter in it.
    const lines = groupItemsIntoLines(
      [item('2', 100, 700), item('2', 500, 700)],
      612,
      1,
    );
    expect(lines.map((l) => l.text)).toEqual(['2']);
  });

  test('dual body lines with narrow gaps still split at the cue-anchored boundary', () => {
    // The real p48 failure in The Last Video Store: long dialogue lines
    // close the inter-column gap, so splitting must use the boundary fixed
    // by the dual-cue line, not per-line gap detection.
    const lines = groupItemsIntoLines(
      [
        item('MARGO', 180, 700), item('DEV', 400, 700),
        item("Let's lock up and then we'll", 150, 688), item('What? Sure, in a minute.', 340, 688),
      ],
      612,
      1,
    );
    expect(lines.map((l) => l.text)).toEqual([
      'MARGO',
      "Let's lock up and then we'll",
      'DEV',
      'What? Sure, in a minute.',
    ]);
  });

  test('a second dual-cue pair starts a new pair of speeches', () => {
    const lines = groupItemsIntoLines(
      [
        item('MARGO', 180, 700), item('DEV', 400, 700),
        item('rewind it again.', 150, 688), item('maybe if you were', 380, 688),
        item("MARGO (CONT'D)", 170, 664), item('DEV', 400, 664),
        item('No. Wait.', 150, 652), item('You wait!', 380, 652),
      ],
      612,
      1,
    );
    expect(lines.map((l) => l.text)).toEqual([
      'MARGO', 'rewind it again.', 'DEV', 'maybe if you were',
      "MARGO (CONT'D)", 'No. Wait.', 'DEV', 'You wait!',
    ]);
  });

  test('two-column title-page furniture is not treated as dual dialogue', () => {
    const lines = groupItemsIntoLines(
      [item('212-555-0134', 72, 700), item('October 28, 2024', 400, 700)],
      612,
      1,
    );
    expect(lines).toHaveLength(1);
    expect(lines[0].text).toBe('212-555-0134 October 28, 2024');
  });

  test('legitimately repeated words at different positions are kept', () => {
    const lines = groupItemsIntoLines(
      [item('No.', 100, 700), item('No.', 140, 700)],
      612,
      1,
    );
    expect(lines[0].text).toBe('No. No.');
  });
});

describe('dual-margin scene numbers', () => {
  test('duplicated margin numbers collapse to one token', () => {
    // Shooting scripts print the scene number in BOTH margins on the
    // slugline's row: "2        2" must become "2", which then attaches
    // to the following scene heading.
    const lines = groupItemsIntoLines(
      [item('2', 54, 700), item('2', 576, 700), item('INT. FOYER - DAY', 108, 676)],
      612,
      1,
    );
    expect(lines.map((l) => l.text)).toEqual(['2', 'INT. FOYER - DAY']);
  });

  test('lettered scene numbers collapse too', () => {
    const lines = groupItemsIntoLines([item('12A.', 54, 700), item('12A.', 570, 700)], 612, 1);
    expect(lines[0].text).toBe('12A.');
  });

  test('distinct numbers on one line are left alone', () => {
    const lines = groupItemsIntoLines([item('1', 54, 700), item('2', 576, 700)], 612, 1);
    expect(lines[0].text).toBe('1 2');
  });
});

describe('whitespace normalization', () => {
  test('padded italic spans and authored doubles collapse to single spaces', () => {
    // Italicized words extract as separate items padded with spaces
    // ("need some  paperwork  here"); HTML collapses them anyway, so the
    // fountain should match what the reader renders.
    const lines = groupItemsIntoLines(
      [item('need some ', 110, 700), item(' paperwork ', 220, 700), item(' here', 340, 700)],
      612,
      1,
    );
    expect(lines[0].text).toBe('need some paperwork here');
  });
});

describe('inline style detection', () => {
  const styled = (
    str: string,
    x: number,
    y: number,
    flags: { italic?: boolean; bold?: boolean; underline?: boolean },
  ) => ({ str, transform: [1, 0, 0, 1, x, y], ...flags });

  test('an italic run inside a plain line gains fountain emphasis markers', () => {
    const lines = groupItemsIntoLines(
      [styled('need some ', 110, 700, {}), styled('paperwork', 220, 700, { italic: true }), styled(' here', 340, 700, {})],
      612,
      1,
    );
    expect(lines[0].text).toBe('need some paperwork here');
    expect(lines[0].styled).toBe('need some *paperwork* here');
  });

  test('bold and bold-italic runs use ** and ***', () => {
    const lines = groupItemsIntoLines(
      [styled('a ', 110, 700, {}), styled('big', 200, 700, { bold: true }), styled(' loud', 300, 700, { bold: true, italic: true })],
      612,
      1,
    );
    expect(lines[0].styled).toBe('a **big** ***loud***');
  });

  test('a fully italic line wraps whole', () => {
    const lines = groupItemsIntoLines([styled('I never wanted that', 200, 700, { italic: true })], 612, 1);
    expect(lines[0].styled).toBe('*I never wanted that*');
  });

  test('punctuation-only styled items never wrap alone', () => {
    const lines = groupItemsIntoLines(
      [styled('by Nora Vance', 110, 700, {}), styled(',', 189, 700, { italic: true }), styled(' the b-side', 195, 700, {})],
      612,
      1,
    );
    expect(lines[0].text).toBe('by Nora Vance, the b-side');
    expect(lines[0].styled).toBeUndefined();
  });

  test('uniform plain lines carry no styled variant', () => {
    const lines = groupItemsIntoLines([styled('Just action.', 110, 700, {})], 612, 1);
    expect(lines[0].styled).toBeUndefined();
  });

  test('an underlined run gains underscores', () => {
    const lines = groupItemsIntoLines(
      [
        styled('This word is ', 110, 700, {}),
        styled('underlined', 200, 700, { underline: true }),
        styled(' with drawn art', 300, 700, {}),
      ],
      612,
      1,
    );
    expect(lines[0].text).toBe('This word is underlined with drawn art');
    expect(lines[0].styled).toBe('This word is _underlined_ with drawn art');
  });

  test('mixed marks nest with the underscore innermost', () => {
    // Not a palindrome: the close mirrors the open. Both renderers unwrap
    // stars first and underscores last, which is exactly this nesting.
    // Two-letter words on purpose: a one-character run is punctuation-only by
    // the guard above and never carries style at all.
    const lines = groupItemsIntoLines(
      [
        styled('bold ', 110, 700, { underline: true, bold: true }),
        styled('slant ', 160, 700, { underline: true, italic: true }),
        styled('both', 220, 700, { underline: true, bold: true, italic: true }),
      ],
      612,
      1,
    );
    expect(lines[0].styled).toBe('**_bold_** *_slant_* ***_both_***');
  });

  test('a punctuation-only underlined item never wraps alone', () => {
    const lines = groupItemsIntoLines(
      [
        styled('by Nora Vance', 110, 700, {}),
        styled(',', 189, 700, { underline: true }),
        styled(' the b-side', 195, 700, {}),
      ],
      612,
      1,
    );
    expect(lines[0].text).toBe('by Nora Vance, the b-side');
    expect(lines[0].styled).toBeUndefined();
  });
});

describe('dual dialogue marking', () => {
  test('the right column cue line carries the dualRight flag', () => {
    const lines = groupItemsIntoLines(
      [
        item('MARGO', 180, 700), item('DEV', 400, 700),
        item('rewind it again.', 150, 688), item('maybe if you were faster', 380, 688),
      ],
      612,
      1,
    );
    expect(lines[0].text).toBe('MARGO');
    expect(lines[0].dualRight).toBeUndefined();
    expect(lines[2].text).toBe('DEV');
    expect(lines[2].dualRight).toBe(true);
    // And each right-column line keeps the y of the row it was printed on,
    // the same as the left-column line beside it. That shared y is the
    // premise of the dual-dialogue picture rule in insertImages
    // (src/parser/index.ts): the right column's y restarts at the top of the
    // exchange, so a picture inside it lands among the left column's lines.
    expect(lines[1].text).toBe('rewind it again.');
    expect(lines[3].text).toBe('maybe if you were faster');
    expect(lines[2].y).toBe(lines[0].y);
    expect(lines[3].y).toBe(lines[1].y);
  });
});

describe('collectUnderlineMarks', () => {
  const PAGE = 612;

  /** Build a one-op operator list drawing a path with bbox [x0,y0,x1,y1]. */
  function paths(
    entries: { bbox: [number, number, number, number]; paint?: number }[],
    prefix: { fn: number; args: unknown[] }[] = [],
  ) {
    const fnArray: number[] = prefix.map((p) => p.fn);
    const argsArray: unknown[] = prefix.map((p) => p.args);
    for (const e of entries) {
      fnArray.push(OPS.constructPath);
      argsArray.push([e.paint ?? OPS.stroke, [], Float32Array.from(e.bbox)]);
    }
    return { fnArray, argsArray };
  }

  /** pdf.js hands back a real Float32Array, so 266.4 arrives as
   * 266.3999938964844. These are geometry assertions, not bit assertions. */
  const rounded = (marks: { x0: number; x1: number; y: number }[]) =>
    marks.map((m) => ({ x0: +m.x0.toFixed(2), x1: +m.x1.toFixed(2), y: +m.y.toFixed(2) }));

  test('a flat, short, filled rule is a mark', () => {
    // The invented fixture's shape: a 72pt x 0.6pt filled rect.
    const marks = collectUnderlineMarks(
      paths([{ bbox: [201.6, 562.0, 273.6, 562.6], paint: OPS.fill }]),
      PAGE,
    );
    expect(rounded(marks)).toEqual([{ x0: 201.6, x1: 273.6, y: 562.3 }]);
  });

  test('a zero-height STROKED rule is a mark too', () => {
    // Every real underline in the local set is a stroke with a zero-height
    // bbox, not a filled rect. Requiring a fill would detect nothing real.
    const marks = collectUnderlineMarks(
      paths([{ bbox: [266.4, 548.5, 346.4, 548.5], paint: OPS.stroke }]),
      PAGE,
    );
    expect(rounded(marks)).toEqual([{ x0: 266.4, x1: 346.4, y: 548.5 }]);
  });

  test('a tall box is not a mark', () => {
    expect(collectUnderlineMarks(paths([{ bbox: [100, 500, 300, 512] }]), PAGE)).toEqual([]);
  });

  test('a hairline shorter than 4pt is not a mark', () => {
    expect(collectUnderlineMarks(paths([{ bbox: [100, 500, 103, 500] }]), PAGE)).toEqual([]);
  });

  test('a page-wide rule is furniture, not a mark', () => {
    // Final Draft's header rules span the full measure; an underline never does.
    expect(collectUnderlineMarks(paths([{ bbox: [0, 744, 612, 744] }]), PAGE)).toEqual([]);
  });

  test('endPath constructs geometry but paints nothing', () => {
    // `W n` clip paths. chromium.pdf draws 34 of them, all full-page boxes.
    expect(
      collectUnderlineMarks(paths([{ bbox: [100, 500, 200, 500], paint: OPS.endPath }]), PAGE),
    ).toEqual([]);
  });

  test('a y-flip CTM is applied, not skipped', () => {
    // final-draft.pdf draws its underlines under [1,0,0,-1,0,792]. Untransformed
    // the mark lands at y=287; the real baseline is at 505.
    const marks = collectUnderlineMarks(
      paths([{ bbox: [222, 287, 390, 287] }], [
        { fn: OPS.save, args: [] },
        { fn: OPS.transform, args: [1, 0, 0, -1, 0, 792] },
      ]),
      PAGE,
    );
    expect(marks).toEqual([{ x0: 222, x1: 390, y: 505 }]);
  });

  test('restore pops the CTM back', () => {
    const marks = collectUnderlineMarks(
      paths([{ bbox: [222, 287, 390, 287] }], [
        { fn: OPS.save, args: [] },
        { fn: OPS.transform, args: [1, 0, 0, -1, 0, 792] },
        { fn: OPS.restore, args: [] },
      ]),
      PAGE,
    );
    expect(marks).toEqual([{ x0: 222, x1: 390, y: 287 }]);
  });

  test('a rotated CTM is skipped rather than guessed at', () => {
    const marks = collectUnderlineMarks(
      paths([{ bbox: [222, 287, 390, 287] }], [
        { fn: OPS.transform, args: [0, 1, -1, 0, 0, 0] },
      ]),
      PAGE,
    );
    expect(marks).toEqual([]);
  });

  test('a malformed operator list yields no marks instead of throwing', () => {
    expect(collectUnderlineMarks({ fnArray: [OPS.constructPath], argsArray: [null] }, PAGE)).toEqual([]);
    expect(collectUnderlineMarks(null as unknown as { fnArray: number[]; argsArray: unknown[] }, PAGE)).toEqual([]);
  });
});

describe('markUnderlinesItem', () => {
  const mark = (y: number, x0 = 100, x1 = 200) => ({ x0, x1, y });

  test('a mark 2pt under the baseline, fully covering the item, matches', () => {
    expect(markUnderlinesItem(mark(498), 100, 200, 500)).toBe(true);
  });

  test('the measured range of real generators all matches', () => {
    // -1.5 (highland p8/p9), -1.7 (torture), -2.0 (final draft), -3.0 (highland p6)
    for (const d of [-1.5, -1.7, -2.0, -3.0]) {
      expect(markUnderlinesItem(mark(500 + d), 100, 200, 500), `d=${d}`).toBe(true);
    }
  });

  test('a strikethrough sits above the baseline and does not match', () => {
    expect(markUnderlinesItem(mark(503.5), 100, 200, 500)).toBe(false);
  });

  test('a table border 6pt below the baseline does not match', () => {
    expect(markUnderlinesItem(mark(494), 100, 200, 500)).toBe(false);
  });

  test("the next line's mark does not match this line", () => {
    // 12pt line spacing: the row below's underline sits at 488 - 2.
    expect(markUnderlinesItem(mark(486), 100, 200, 500)).toBe(false);
  });

  test('a mark covering less than 60% of the item does not match', () => {
    // highland p8: a 36pt rule under a 129.5pt item. Marking the whole item
    // underlined would put _ around 13 characters that are not underlined.
    expect(markUnderlinesItem(mark(498, 100, 136), 100, 229.5, 500)).toBe(false);
  });

  test('a mark covering 60% or more does match', () => {
    expect(markUnderlinesItem(mark(498, 100, 160), 100, 200, 500)).toBe(true);
  });

  test('a zero-width item never matches', () => {
    expect(markUnderlinesItem(mark(498), 100, 100, 500)).toBe(false);
  });
});

describe('familyBucket', () => {
  test('screenplay monospace faces bucket as mono', () => {
    for (const n of ['Courier', 'CourierPrime-Bold', 'AAAAAB+CourierFinalDraft',
                     'WXPDAA+CourierNewPSMT', 'LetterGothic', 'Prestige Elite',
                     'Andale Mono']) {
      expect(familyBucket(n), n).toBe('mono');
    }
  });

  test('Century Gothic is a sans, not a mono', () => {
    // The trap the spec calls out: match the JOINED name "lettergothic",
    // never bare "gothic".
    expect(familyBucket('CenturyGothic')).toBe('sans');
  });

  test('serif faces bucket as serif', () => {
    for (const n of ['Times-Roman', 'TimesNewRomanPSMT', 'Georgia', 'Garamond',
                     'Palatino-Roman']) {
      expect(familyBucket(n), n).toBe('serif');
    }
  });

  test('a -Roman weight suffix does not make a sans into a serif', () => {
    // "-Roman" is PostScript for the REGULAR weight. Times matches on "times"
    // anyway, so bare "roman" buys nothing and misbuckets these.
    for (const n of ['Helvetica-Roman', 'AvenirNext-Roman', 'Frutiger-Roman']) {
      expect(familyBucket(n), n).toBe('sans');
    }
  });

  test('handwriting faces bucket as cursive', () => {
    for (const n of ['BrushScriptMT', 'BradleyHandITC', 'Comic Sans MS']) {
      expect(familyBucket(n), n).toBe('cursive');
    }
  });

  test('everything else is sans', () => {
    for (const n of ['AvenirNext-Bold', 'Helvetica', 'Scream', 'Arial-BoldMT']) {
      expect(familyBucket(n), n).toBe('sans');
    }
  });

  test('style tokens never decide the bucket', () => {
    expect(familyBucket('Courier-BoldOblique')).toBe('mono');
    expect(familyBucket('Georgia-Italic')).toBe('serif');
  });

  test('an empty or missing name has no bucket', () => {
    expect(familyBucket('')).toBeUndefined();
    expect(familyBucket('AAAAAB+')).toBeUndefined();
  });
});

describe('stampLineFmt', () => {
  const line = (fonts: FontRun[]): RawLine =>
    ({ text: 'x', indent: 10, y: 700, pageNum: 1, fonts });

  const body = (n: number) => line([{ bucket: 'mono', size: 12, chars: n }]);

  test('a uniform document produces no fmt anywhere', () => {
    const lines = [body(500), body(500), body(500)];
    stampLineFmt(lines);
    expect(lines.map((l) => l.fmt)).toEqual([undefined, undefined, undefined]);
  });

  test('a line in another family gets a family fmt', () => {
    const shifted = line([{ bucket: 'sans', size: 12, chars: 30 }]);
    const lines = [body(500), shifted, body(500)];
    stampLineFmt(lines);
    expect(shifted.fmt).toEqual({ family: 'sans', size: undefined });
  });

  test('the three size steps come off the ratio, both directions', () => {
    const smaller = line([{ bucket: 'mono', size: 10, chars: 30 }]);   // 0.83
    const bigger = line([{ bucket: 'mono', size: 15, chars: 30 }]);    // 1.25
    const biggest = line([{ bucket: 'mono', size: 18, chars: 30 }]);   // 1.50
    const lines = [body(500), smaller, bigger, biggest, body(500)];
    stampLineFmt(lines);
    expect(smaller.fmt).toEqual({ family: undefined, size: '-1' });
    expect(bigger.fmt).toEqual({ family: undefined, size: '+1' });
    expect(biggest.fmt).toEqual({ family: undefined, size: '+2' });
  });

  test('a shift in both arms reports both', () => {
    const both = line([{ bucket: 'sans', size: 18, chars: 30 }]);
    const lines = [body(500), both];
    stampLineFmt(lines);
    expect(both.fmt).toEqual({ family: 'sans', size: '+2' });
  });

  test('float jitter under 1.5pt never fires', () => {
    // 13.4/12 = 1.117 is under the ratio floor anyway; the absolute delta is
    // the belt to the ratio's braces.
    const jittery = line([{ bucket: 'mono', size: 13.4, chars: 30 }]);
    const lines = [body(500), jittery];
    stampLineFmt(lines);
    expect(jittery.fmt).toBeUndefined();
  });

  test('a ratio between the steps is not a step', () => {
    // 13.5/12 = 1.125: bigger than jitter, smaller than +1's 1.15 floor.
    const between = line([{ bucket: 'mono', size: 13.5, chars: 30 }]);
    stampLineFmt([body(500), between]);
    expect(between.fmt).toBeUndefined();
  });

  test('a line only half in the deviant font gets nothing', () => {
    const mixed = line([
      { bucket: 'mono', size: 12, chars: 20 },
      { bucket: 'sans', size: 12, chars: 20 },
    ]);
    stampLineFmt([body(500), mixed]);
    expect(mixed.fmt).toBeUndefined();
  });

  test('80% agreement is enough', () => {
    const mostly = line([
      { bucket: 'mono', size: 12, chars: 2 },
      { bucket: 'sans', size: 12, chars: 8 },
    ]);
    stampLineFmt([body(500), mostly]);
    expect(mostly.fmt).toEqual({ family: 'sans', size: undefined });
  });

  test('the dominant is by character weight, not by line count', () => {
    // Many short sans lines lose to one long mono block.
    const sansLines = Array.from({ length: 20 }, () =>
      line([{ bucket: 'sans', size: 12, chars: 5 }]));
    const monoLine = line([{ bucket: 'mono', size: 12, chars: 900 }]);
    stampLineFmt([...sansLines, monoLine]);
    expect(monoLine.fmt).toBeUndefined();
    expect(sansLines[0].fmt).toEqual({ family: 'sans', size: undefined });
  });

  test('lines with no resolved fonts are left alone', () => {
    const bare: RawLine = { text: 'x', indent: 10, y: 700, pageNum: 1 };
    stampLineFmt([body(500), bare]);
    expect(bare.fmt).toBeUndefined();
  });
});

describe('font runs on extracted lines', () => {
  const sized = (str: string, x: number, y: number, size: number, bucket?: string) =>
    ({ str, transform: [size, 0, 0, size, x, y], bucket });

  test('a line records its per-run bucket and size, by trimmed length', () => {
    const lines = groupItemsIntoLines(
      [sized('CHYRON: ', 110, 700, 12, 'mono'), sized('LIVE', 170, 700, 18, 'sans')],
      612,
      1,
    );
    expect(lines[0].fonts).toEqual([
      { bucket: 'mono', size: 12, chars: 7 },
      { bucket: 'sans', size: 18, chars: 4 },
    ]);
  });

  test('adjacent runs with the same bucket and size merge', () => {
    const lines = groupItemsIntoLines(
      [sized('one ', 110, 700, 12, 'mono'), sized('two', 150, 700, 12, 'mono')],
      612,
      1,
    );
    expect(lines[0].fonts).toEqual([{ bucket: 'mono', size: 12, chars: 6 }]);
  });

  test('an item whose font never resolved contributes nothing', () => {
    const lines = groupItemsIntoLines(
      [sized('seen', 110, 700, 12, 'mono'), sized('unseen', 160, 700, 12, undefined)],
      612,
      1,
    );
    expect(lines[0].fonts).toEqual([{ bucket: 'mono', size: 12, chars: 4 }]);
  });

  test('a negative vertical scale still reports a positive size', () => {
    const flipped = { str: 'flipped', transform: [12, 0, 0, -12, 110, 700], bucket: 'mono' };
    expect(groupItemsIntoLines([flipped], 612, 1)[0].fonts).toEqual([
      { bucket: 'mono', size: 12, chars: 7 },
    ]);
  });
});

describe('extractDocument progress', () => {
  // The app showed an indeterminate spinner during conversion because the
  // engine reported nothing until it was finished. Page extraction is the
  // long stage and already counts pages, so it is where a real fraction
  // comes from.
  const bytes = async () =>
    new Uint8Array(await Bun.file('tests/fixtures/screenplay.pdf').arrayBuffer());

  test('reports one progress tick per page, with the total', async () => {
    const ticks: { page: number; pages: number }[] = [];
    // getDocument transfers the buffer, so each call needs its own bytes.
    const { pageCount } = await extractDocument(await bytes(), undefined, (page, pages) =>
      ticks.push({ page, pages }),
    );

    expect(pageCount).toBe(5);
    expect(ticks).toHaveLength(5);
    expect(ticks.map((t) => t.page)).toEqual([1, 2, 3, 4, 5]);
    // Every tick carries the same total so a fraction is computable from
    // the first one, without waiting to learn the denominator.
    expect(ticks.every((t) => t.pages === 5)).toBe(true);
  });

  test('is optional: extraction works with no callback', async () => {
    const { pageCount, lines } = await extractDocument(await bytes());
    expect(pageCount).toBe(5);
    expect(lines.length).toBeGreaterThan(0);
  });
});

// ── dual dialogue: the boundary must refine before it judges ─────────
//
// Geometry lifted from a real script (page width 612): the right column's
// CUE sits at x=424 but its BODY starts at x=332, an offset of 15% of the
// page rather than the 13% the opening boundary assumes. That puts the
// right column's own dialogue LEFT of the boundary while it still runs
// past it, so the straddle test read a cleanly split two-column line as a
// full-width one and ended the region on its first body line.
//
// What the reader got was both cues demoted to action and the two
// speeches concatenated into one sentence: "Really great. Just okay."
describe('dual dialogue body lines', () => {
  const dual = () => groupItemsIntoLines([
    item('LOU', 200, 219), item('SYD', 424, 219),
    item('Really great.', 108, 207), item('Just okay.', 332, 207),
  ], 612, 6);

  test('the two speeches are not concatenated into one line', () => {
    expect(dual().map((l) => l.text)).not.toContain('Really great. Just okay.');
  });

  test('each column keeps its own speech, left column first', () => {
    expect(dual().map((l) => l.text)).toEqual(['LOU', 'Really great.', 'SYD', 'Just okay.']);
  });

  test("the right column's cue is marked for fountain's ^ syntax", () => {
    const lines = dual();
    expect(lines.find((l) => l.text === 'SYD')?.dualRight).toBe(true);
    expect(lines.find((l) => l.text === 'LOU')?.dualRight).toBeUndefined();
  });
});

// ── phantom spaces invented by pdf.js ────────────────────────────────
//
// Found 2026-09-16 in a Final Draft script that had been re-saved through
// Quartz. The re-encode left one anomalous kerning adjustment roughly once
// a page: every glyph pair on a line carries +17, and one carries about
// -110, which widens that single gap by ~1.5pt. pdf.js reads a gap that
// size as a word space and puts one in the string it hands us, so "these"
// arrives as "thes e". The PDF contains no space there at all.
//
// The repair reads the glyph run, where a REAL space is a glyph of its own
// and an invented one is not present at all. That is what makes this safe
// against a writer who spaced a word out deliberately: their spaces are
// characters and survive untouched.
describe('repairPhantomSpaces', () => {
  const SHOW_TEXT = 44; // OPS.showText
  const g = (s: string) => [...s].map((c) => ({ unicode: c, width: 600 }));
  const opsFor = (...runs: string[]) => ({
    fnArray: runs.map(() => SHOW_TEXT),
    argsArray: runs.map((r) => [g(r)]),
  });
  // 7.0 per cell, the tracked Courier this bug appears in.
  const it = (str: string, cells: number) => ({
    str,
    transform: [1, 0, 0, 1, 100, 700],
    width: cells * 7,
  });

  test('a space with no glyph behind it is removed', () => {
    // "these men." is 10 glyphs of ink but 11 characters of string.
    // Three clean lines alongside, because the cell width is a median over
    // the page and a page with almost nothing on it should not produce one.
    const items = [
      it('sit well with thes e men.', 24),
      it('a normal line of dialogue here', 30),
      it('another perfectly ordinary line', 31),
      it('and a third one for the median', 30),
    ];
    const fixed = repairPhantomSpaces(
      items,
      opsFor(
        'sit well with these men.',
        'a normal line of dialogue here',
        'another perfectly ordinary line',
        'and a third one for the median',
      ),
    );
    expect(fixed).toBe(1);
    expect(items[0].str).toBe('sit well with these men.');
    expect(items[1].str).toBe('a normal line of dialogue here');
  });

  test('a glyph run with a trailing space still repairs the item', () => {
    // The defect in the first cut of this, found on the real script: the
    // glyph run for a line usually ends with a trailing space, so it is the
    // SAME LENGTH as the damaged item even though it has one less space in
    // the middle. A raw length comparison then reads "no improvement" and
    // skips the repair. Three of the four known cases were skipped this way
    // and only the one run without a trailing space was fixed.
    const items = [
      it('as we sl owly PAN AWAY.', 22),
      it('a normal line of dialogue here', 30),
      it('another perfectly ordinary line', 31),
      it('and a third one for the median', 30),
    ];
    const fixed = repairPhantomSpaces(
      items,
      opsFor(
        'as we slowly PAN AWAY. ',
        'a normal line of dialogue here',
        'another perfectly ordinary line',
        'and a third one for the median',
      ),
    );
    expect(fixed).toBe(1);
    expect(items[0].str).toBe('as we slowly PAN AWAY.');
  });

  test('a deliberately spaced-out word is left alone', () => {
    // The writer typed "s l o w l y". Every space is a real glyph, so the
    // ink matches the string and nothing is flagged. This is the assurance
    // that the repair cannot second-guess an author.
    const items = [
      it('we go s l o w l y now', 21),
      it('a normal line of dialogue here', 30),
      it('another perfectly ordinary line', 31),
      it('and a third one for the median', 30),
    ];
    const fixed = repairPhantomSpaces(
      items,
      opsFor(
        'we go s l o w l y now',
        'a normal line of dialogue here',
        'another perfectly ordinary line',
        'and a third one for the median',
      ),
    );
    expect(fixed).toBe(0);
    expect(items[0].str).toBe('we go s l o w l y now');
  });

  test('an ambiguous match is refused rather than guessed', () => {
    // Two runs on the page collapse to the same key. Picking one would be
    // a coin flip, so neither is applied.
    const items = [
      it('thes e men.', 10),
      it('a normal line of dialogue here', 30),
      it('another perfectly ordinary line', 31),
      it('and a third one for the median', 30),
    ];
    const fixed = repairPhantomSpaces(
      items,
      opsFor(
        'these men.',
        'these men.',
        'a normal line of dialogue here',
        'another perfectly ordinary line',
        'and a third one for the median',
      ),
    );
    expect(fixed).toBe(0);
    expect(items[0].str).toBe('thes e men.');
  });

  test('an item whose ink matches its string is never touched', () => {
    const items = [
      it('these men.', 10),
      it('a normal line of dialogue here', 30),
      it('another perfectly ordinary line', 31),
      it('and a third one for the median', 30),
    ];
    expect(
      repairPhantomSpaces(
        items,
        opsFor(
          'these men.',
          'a normal line of dialogue here',
          'another perfectly ordinary line',
          'and a third one for the median',
        ),
      ),
    ).toBe(0);
  });

  test('no operator list means no repair, not a crash', () => {
    const items = [it('thes e men.', 10)];
    expect(repairPhantomSpaces(items, null)).toBe(0);
    expect(items[0].str).toBe('thes e men.');
  });
});

// ── the repair is counted, never silent ──────────────────────────────
//
// repairPhantomSpaces changes an author's text. The owner's first instinct
// was to make it opt-in, and the concern behind that is right: we should
// not quietly rewrite someone's script. But a toggle answers it badly,
// because it defaults to something — off, and people see broken words and
// never find the setting; on, and the switch exists for a case nobody can
// name — and because this is a correctness repair, not a preference: the
// PDF says "these", we were writing "thes e".
//
// So it reports instead of asking. The count rides out with the result, the
// app can say "repaired 94 spacing artifacts", and a number that suddenly
// jumps is how anyone would notice it misfiring on a script.
describe('extractDocument reports what it repaired', () => {
  test('the count is part of the extraction result', async () => {
    const pdf = new Uint8Array(await Bun.file('tests/fixtures/screenplay.pdf').arrayBuffer());
    const out = await extractDocument(pdf);
    // The committed fixture is clean, so the honest answer here is zero —
    // and zero has to be REPORTED rather than absent, or a caller cannot
    // tell "nothing to repair" from "this engine does not count".
    expect(out.spacingRepairs).toBe(0);
    expect(out.pageCount).toBeGreaterThan(0);
  });

  test('a PDF that needs repairs reports a non-zero count', async () => {
    // The committed fixtures are all clean, so the only honest source for
    // this is a real file. Skips rather than lies when the corpus is absent
    // (it is gitignored and does not exist in CI).
    const real = `${process.env.HOME}/Downloads/One Day This Could All Be Yours by Perry Janes.pdf`;
    if (!existsSync(real)) return;
    const out = await extractDocument(new Uint8Array(await Bun.file(real).arrayBuffer()));
    expect(out.spacingRepairs).toBeGreaterThan(50);
  });
});
