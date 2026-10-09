// The Final Draft (.fdx) renderer: Fountain in, an editable screenplay out.
// Every case is a small inline Fountain string, read back through a strict
// XML reader so a malformed document fails here rather than in someone's
// copy of Final Draft.
import { describe, expect, test } from 'bun:test';
import { FDX_VERSION, fountainToFdx, textRuns } from '../src/fdx/fdx';
import { childElements, parseXml, textOf, type XmlElement } from './helpers/mini-xml';

interface Para {
  type: string | undefined;
  attrs: Record<string, string>;
  runs: { style: string | undefined; text: string }[];
  text: string;
  el: XmlElement;
}

function toPara(el: XmlElement): Para {
  const runs = childElements(el, 'Text').map((t) => ({ style: t.attrs.Style, text: textOf(t) }));
  return { type: el.attrs.Type, attrs: el.attrs, runs, text: runs.map((r) => r.text).join(''), el };
}

function doc(fountain: string): XmlElement {
  return parseXml(fountainToFdx(fountain));
}

function body(fountain: string): Para[] {
  const content = childElements(doc(fountain), 'Content')[0];
  return childElements(content, 'Paragraph').map(toPara);
}

function titlePage(fountain: string): Para[] | null {
  const tp = childElements(doc(fountain), 'TitlePage')[0];
  if (!tp) return null;
  return childElements(childElements(tp, 'Content')[0], 'Paragraph').map(toPara);
}

describe('the document shell', () => {
  test('an XML declaration, then a FinalDraft root at the one version constant', () => {
    const fdx = fountainToFdx('INT. ROOM - DAY\n');
    expect(fdx.startsWith('<?xml version="1.0" encoding="UTF-8" standalone="no" ?>\n')).toBe(true);
    const root = parseXml(fdx);
    expect(root.name).toBe('FinalDraft');
    expect(root.attrs).toEqual({ DocumentType: 'Script', Template: 'No', Version: String(FDX_VERSION) });
    expect(FDX_VERSION).toBe(2);
  });

  test('minimal: Content and TitlePage only, no settings or layout blocks', () => {
    const root = doc('Title: A Thing\n\nINT. ROOM - DAY\n');
    expect(childElements(root).map((c) => c.name)).toEqual(['Content', 'TitlePage']);
  });

  test('no title page tokens: no TitlePage at all', () => {
    expect(titlePage('INT. ROOM - DAY\n\nSome action.\n')).toBeNull();
  });

  test('an empty script is still a well-formed document', () => {
    expect(body('')).toEqual([]);
  });
});

describe('paragraph types', () => {
  test('scene heading, action, character, parenthetical, dialogue, transition', () => {
    const p = body([
      'INT. KITCHEN - NIGHT',
      '',
      'The kettle screams.',
      '',
      'MARGO',
      '(to herself)',
      'Not now.',
      '',
      'CUT TO:',
      '',
    ].join('\n'));
    expect(p.map((x) => [x.type, x.text])).toEqual([
      ['Scene Heading', 'INT. KITCHEN - NIGHT'],
      ['Action', 'The kettle screams.'],
      ['Character', 'MARGO'],
      ['Parenthetical', '(to herself)'],
      ['Dialogue', 'Not now.'],
      ['Transition', 'CUT TO:'],
    ]);
  });

  test('a scene number rides as Number and leaves the heading text', () => {
    const [h] = body('INT. HOUSE - DAY #12A#\n');
    expect(h.type).toBe('Scene Heading');
    expect(h.attrs.Number).toBe('12A');
    expect(h.text).toBe('INT. HOUSE - DAY');
  });

  test('an unnumbered heading carries no Number attribute', () => {
    const [h] = body('EXT. FIELD - DAY\n');
    expect('Number' in h.attrs).toBe(false);
  });

  test('a mini-slug (dot-forced secondary heading) is a Shot, without the dot', () => {
    const p = body('INT. HOUSE - DAY\n\n.LATER\n\nShe waits.\n');
    expect(p.map((x) => [x.type, x.text])).toEqual([
      ['Scene Heading', 'INT. HOUSE - DAY'],
      ['Shot', 'LATER'],
      ['Action', 'She waits.'],
    ]);
  });

  test('a mini-slug keeps its shooting-script number too', () => {
    const [, shot] = body('INT. HOUSE - DAY\n\n.THE HALL #7#\n');
    expect(shot.type).toBe('Shot');
    expect(shot.attrs.Number).toBe('7');
  });

  test('forced elements lose their sigils', () => {
    const p = body([
      '.FORCED HEADING OF A SCENE',
      '',
      '!LOUD ACTION THAT LOOKS LIKE A CUE',
      '',
      '@McCLANE',
      'Yippee.',
      '',
      '> SMASH CUT',
      '',
    ].join('\n'));
    expect(p.map((x) => x.text)).toEqual([
      'FORCED HEADING OF A SCENE',
      'LOUD ACTION THAT LOOKS LIKE A CUE',
      'McCLANE',
      'Yippee.',
      'SMASH CUT',
    ]);
    expect(p.map((x) => x.type)).toEqual(['Shot', 'Action', 'Character', 'Dialogue', 'Transition']);
  });

  test('character extensions stay in the cue', () => {
    const [cue] = body("MARGO (V.O.) (CONT'D)\nHello.\n");
    expect(cue.type).toBe('Character');
    expect(cue.text).toBe("MARGO (V.O.) (CONT'D)");
  });

  test('centered text is Action, aligned center, without the brackets', () => {
    const [c] = body('> THE END <\n');
    expect(c.type).toBe('Action');
    expect(c.attrs.Alignment).toBe('Center');
    expect(c.text).toBe('THE END');
  });

  test('ordinary action carries no Alignment', () => {
    const [a] = body('She runs.\n');
    expect('Alignment' in a.attrs).toBe(false);
  });

  test('lyrics are Lyrics, one paragraph per line, without the tildes', () => {
    const p = body('~Row, row, row\n~your boat\n');
    expect(p.map((x) => [x.type, x.text])).toEqual([
      ['Lyrics', 'Row, row, row'],
      ['Lyrics', 'your boat'],
    ]);
  });

  test('a lyric inside a speech stays inside it', () => {
    const p = body('MARGO\nI sing:\n~la la la\n');
    expect(p.map((x) => x.type)).toEqual(['Character', 'Dialogue', 'Lyrics']);
  });

  test('a multi-line token becomes one paragraph per line, blank lines dropped', () => {
    const p = body('MARGO\nFirst line\nsecond line\n\nAction one\naction two\n');
    expect(p.map((x) => [x.type, x.text])).toEqual([
      ['Character', 'MARGO'],
      ['Dialogue', 'First line'],
      ['Dialogue', 'second line'],
      ['Action', 'Action one'],
      ['Action', 'action two'],
    ]);
  });
});

describe('what never reaches the FDX', () => {
  test('notes, including Screepub’s own fmt notes, are stripped', () => {
    const p = body([
      '[[fmt: sans +1]]',
      'A shout across the room.',
      '',
      'MARGO',
      '[[fmt: mono]] Typed words [[an aside]] here.',
      '',
      '[[a note on its own]]',
      '',
    ].join('\n'));
    expect(p.map((x) => [x.type, x.text])).toEqual([
      ['Action', 'A shout across the room.'],
      ['Character', 'MARGO'],
      ['Dialogue', 'Typed words here.'],
    ]);
    expect(fountainToFdx('Words [[note]] more.\n')).not.toContain('[[');
  });

  test('a picture note never reaches it, standalone, full-page or glued to a line', () => {
    // Pictures carried out of a PDF are recorded as [[img: ...]] notes
    // (registry #19). An FDX is a script to edit, and a picture has no
    // paragraph type there, so the note goes the way every note goes: the
    // FDX shows neither the note nor the file it names.
    const fountain = [
      '[[img: p001-01.png w=100 full]]',
      '',
      'INT. ARCHIVE - DAY',
      '',
      'She holds up a panel.',
      '',
      '[[img: p002-01.png w=70]]',
      '',
      'MARGO',
      'Look at this. [[img: p002-02.png w=40]]',
      '',
    ].join('\n');
    const fdx = fountainToFdx(fountain);
    expect(fdx).not.toContain('img:');
    expect(fdx).not.toContain('.png');
    expect(body(fountain).map((x) => [x.type, x.text])).toEqual([
      ['Scene Heading', 'INT. ARCHIVE - DAY'],
      ['Action', 'She holds up a panel.'],
      ['Character', 'MARGO'],
      ['Dialogue', 'Look at this.'],
    ]);
  });

  test('boneyard, sections and synopses (and Screepub’s page markers) are dropped', () => {
    const p = body([
      '# Act One',
      '',
      '= The setup',
      '',
      'INT. ROOM - DAY',
      '',
      '/* a cut scene',
      'that is gone */',
      '',
      '= pg 2',
      '',
      'She sits.',
      '',
    ].join('\n'));
    expect(p.map((x) => [x.type, x.text])).toEqual([
      ['Scene Heading', 'INT. ROOM - DAY'],
      ['Action', 'She sits.'],
    ]);
  });
});

describe('page breaks', () => {
  test('=== sets StartsNewPage on the next paragraph only', () => {
    const p = body('One.\n\n===\n\nTwo.\n\nThree.\n');
    expect(p.map((x): [string, string | undefined] => [x.text, x.attrs.StartsNewPage])).toEqual([
      ['One.', undefined],
      ['Two.', 'Yes'],
      ['Three.', undefined],
    ]);
  });

  test('a trailing page break with nothing after it leaves no trace', () => {
    const p = body('One.\n\n===\n');
    expect(p).toHaveLength(1);
    expect(p[0].attrs.StartsNewPage).toBeUndefined();
  });

  test('a page break before dual dialogue lands on the wrapper', () => {
    const p = body('===\n\nBOB\nLeft.\n\nALICE ^\nRight.\n');
    expect(p).toHaveLength(1);
    expect(p[0].attrs.StartsNewPage).toBe('Yes');
  });
});

describe('dual dialogue', () => {
  test('both speeches nest inside one Paragraph > DualDialogue', () => {
    const p = body('INT. ROOM - DAY\n\nBOB\n(fast)\nLeft.\n\nALICE ^\nRight.\n\nThey stop.\n');
    expect(p.map((x) => x.type)).toEqual(['Scene Heading', 'General', 'Action']);
    const wrapper = p[1].el;
    // Nothing but the DualDialogue element inside the wrapper: no stray text.
    expect(wrapper.children).toHaveLength(1);
    const dual = childElements(wrapper, 'DualDialogue')[0];
    expect(dual).toBeDefined();
    expect(dual.children.every((c) => typeof c !== 'string')).toBe(true);
    const inner = childElements(dual, 'Paragraph').map(toPara);
    expect(inner.map((x) => [x.type, x.text])).toEqual([
      ['Character', 'BOB'],
      ['Parenthetical', '(fast)'],
      ['Dialogue', 'Left.'],
      ['Character', 'ALICE'],
      ['Dialogue', 'Right.'],
    ]);
  });
});

describe('emphasis', () => {
  test('textRuns splits Fountain emphasis into styled runs', () => {
    expect(textRuns('plain *it* **bo** _un_ end')).toEqual([
      { text: 'plain ' },
      { text: 'it', style: 'Italic' },
      { text: ' ' },
      { text: 'bo', style: 'Bold' },
      { text: ' ' },
      { text: 'un', style: 'Underline' },
      { text: ' end' },
    ]);
  });

  test('styles combine as Bold+Italic+Underline, in that order', () => {
    expect(textRuns('***both***')).toEqual([{ text: 'both', style: 'Bold+Italic' }]);
    expect(textRuns('_***all three***_')).toEqual([{ text: 'all three', style: 'Bold+Italic+Underline' }]);
    expect(textRuns('*_it under_*')).toEqual([{ text: 'it under', style: 'Italic+Underline' }]);
    expect(textRuns('**bold *and it* back**')).toEqual([
      { text: 'bold ', style: 'Bold' },
      { text: 'and it', style: 'Bold+Italic' },
      { text: ' back', style: 'Bold' },
    ]);
  });

  test('unpaired or spaced markers stay literal', () => {
    expect(textRuns('2 * 3 = 6')).toEqual([{ text: '2 * 3 = 6' }]);
    expect(textRuns('snake_case only')).toEqual([{ text: 'snake_case only' }]);
  });

  test('backslash-escaped markers are literal and lose the backslash', () => {
    expect(textRuns('a \\*not italic\\* b')).toEqual([{ text: 'a *not italic* b' }]);
  });

  test('runs land as Text elements; plain runs carry no Style', () => {
    const [a] = body('He is *very* **sure**.\n');
    expect(a.runs).toEqual([
      { style: undefined, text: 'He is ' },
      { style: 'Italic', text: 'very' },
      { style: undefined, text: ' ' },
      { style: 'Bold', text: 'sure' },
      { style: undefined, text: '.' },
    ]);
  });

  test('no whitespace between or around the Text runs of a paragraph', () => {
    const fdx = fountainToFdx('He is *very* sure.\n');
    expect(fdx).toContain('<Paragraph Type="Action"><Text>He is </Text><Text Style="Italic">very</Text><Text> sure.</Text></Paragraph>');
  });
});

describe('escaping', () => {
  test('&, <, > and " survive a round trip through the XML', () => {
    const [a] = body('Tom & Jerry say "<hi>" > loudly.\n');
    expect(a.text).toBe('Tom & Jerry say "<hi>" > loudly.');
    const fdx = fountainToFdx('Tom & Jerry say "<hi>".\n');
    expect(fdx).toContain('Tom &amp; Jerry say &quot;&lt;hi&gt;&quot;.');
  });

  test('characters XML 1.0 forbids are dropped, not written', () => {
    const fdx = fountainToFdx('Bell\u0007 and form\u000C feed and \uFFFE end.\n');
    expect(() => parseXml(fdx)).not.toThrow();
    const [a] = body('Bell\u0007 and form\u000C feed and \uFFFE end.\n');
    expect(a.text).toBe('Bell and form feed and  end.');
  });

  test('a lone surrogate is dropped too', () => {
    const [a] = body('Broken \uD800 pair.\n');
    expect(a.text).toBe('Broken  pair.');
  });

  test('a scene number with markup characters is escaped in the attribute', () => {
    const fdx = fountainToFdx('INT. HOUSE - DAY #1-A.#\n');
    expect(() => parseXml(fdx)).not.toThrow();
  });
});

describe('the title page', () => {
  const SOURCE = [
    'Title: **THE BIG** _Thing_',
    '    Part Two',
    'Credit: Written by',
    'Author: A. Writer',
    'Source: Based on a story',
    'Draft date: 1 January 2020',
    'Contact:',
    '    A. Agent',
    '    555-0100',
    '',
    'INT. ROOM - DAY',
    '',
  ].join('\n');

  test('title, credit, author and source are centered, in that order, near the top', () => {
    const p = titlePage(SOURCE)!;
    const centered = p.filter((x) => x.attrs.Alignment === 'Center' && x.text !== '');
    expect(centered.map((x) => x.text)).toEqual([
      'THE BIG Thing',
      'Part Two',
      'Written by',
      'A. Writer',
      'Based on a story',
    ]);
    // The title keeps its own emphasis.
    expect(centered[0].runs).toEqual([
      { style: 'Bold', text: 'THE BIG' },
      { style: undefined, text: ' ' },
      { style: 'Underline', text: 'Thing' },
    ]);
    // Blank lines push the block down the page.
    const firstText = p.findIndex((x) => x.text !== '');
    expect(firstText).toBeGreaterThanOrEqual(10);
  });

  test('contact and draft date sit at the bottom, left aligned, after the centered block', () => {
    const p = titlePage(SOURCE)!;
    const lastCentered = p.map((x) => x.attrs.Alignment === 'Center' && x.text !== '').lastIndexOf(true);
    const bottom = p.slice(lastCentered + 1).filter((x) => x.text !== '');
    expect(bottom.map((x) => x.text)).toEqual(['A. Agent', '555-0100', '1 January 2020']);
    expect(bottom.every((x) => x.attrs.Alignment === 'Left')).toBe(true);
    // Below the middle of a ~55-line page.
    const firstBottom = p.findIndex((x) => x.text === 'A. Agent');
    expect(firstBottom).toBeGreaterThan(35);
  });

  test('title page paragraphs carry no element Type, like Final Draft’s own', () => {
    for (const x of titlePage(SOURCE)!) expect(x.type).toBeUndefined();
  });

  test('title page tokens never leak into the script body', () => {
    expect(body(SOURCE).map((x) => x.type)).toEqual(['Scene Heading']);
  });

  test('a note on the title page is stripped there too', () => {
    const p = titlePage('Title: Draft [[check this]] One\n\nINT. ROOM - DAY\n')!;
    expect(p.filter((x) => x.text !== '').map((x) => x.text)).toEqual(['Draft One']);
  });
});
