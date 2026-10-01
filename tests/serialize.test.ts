import { describe, test, expect } from 'bun:test';
import { Fountain } from 'fountain-js';
import { extractTitleMeta, toFountain } from '../src/fountain/serialize';
import { parseImgNote } from '../src/fountain/notes';
import { tokensToBody } from '../src/epub/html';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';
import type { ScreenplayElement, ParsedScreenplay } from '../src/parser/types';

let idCounter = 0;
function el(over: Partial<ScreenplayElement> & { text: string; type: ScreenplayElement['type'] }): ScreenplayElement {
  return {
    id: `t${idCounter++}`,
    pageNum: 1,
    isTitlePage: false,
    isReadable: true,
    ...over,
  };
}

function screenplay(elements: ScreenplayElement[]): ParsedScreenplay {
  return { elements, characters: [], scenes: [], pageCount: 1 };
}

// ── extractTitleMeta ─────────────────────────────────────

describe('extractTitleMeta', () => {
  test('title joins readable elements before by-line, author follows it', () => {
    const meta = extractTitleMeta([
      el({ text: 'MOONQUAKE', type: 'action', isTitlePage: true }),
      el({ text: 'JAMBOREE', type: 'action', isTitlePage: true }),
      el({ text: 'Written by', type: 'action', isTitlePage: true }),
      el({ text: 'Fenwick Sprocket & Tallulah Quibble', type: 'action', isTitlePage: true }),
      el({ text: '© 2025 Placeholder Pictures', type: 'action', isTitlePage: true, isReadable: false }),
      el({ text: 'INT. HOUSE - DAY', type: 'scene', pageNum: 2 }),
    ]);
    expect(meta.title).toBe('Moonquake Jamboree');
    expect(meta.author).toBe('Fenwick Sprocket & Tallulah Quibble');
  });

  test('inline by-line "written by / Name" yields author from same element', () => {
    const meta = extractTitleMeta([
      el({ text: 'ZEPPELIN PICNIC', type: 'action', isTitlePage: true }),
      el({ text: 'written by Barnaby Fictitious', type: 'action', isTitlePage: true }),
    ]);
    expect(meta.title).toBe('Zeppelin Picnic');
    expect(meta.author).toBe('Barnaby Fictitious');
  });

  test('no title-page elements yields empty meta', () => {
    const meta = extractTitleMeta([el({ text: 'INT. HOUSE - DAY', type: 'scene' })]);
    expect(meta.title).toBeUndefined();
    expect(meta.author).toBeUndefined();
  });

  test('non-readable contact info is never the author', () => {
    const meta = extractTitleMeta([
      el({ text: 'GARGOYLE BRUNCH', type: 'action', isTitlePage: true }),
      el({ text: 'By', type: 'action', isTitlePage: true }),
      el({ text: 'Imaginaria Nobody', type: 'action', isTitlePage: true }),
      el({ text: 'imaginaria@example.com', type: 'action', isTitlePage: true, isReadable: false }),
    ]);
    expect(meta.author).toBe('Imaginaria Nobody');
  });
});

// ── toFountain ───────────────────────────────────────────

describe('toFountain', () => {
  test('emits title block when meta present', () => {
    const out = toFountain(screenplay([el({ text: 'INT. HOUSE - DAY', type: 'scene' })]), {
      title: 'My Script',
      author: 'Jane Doe',
    });
    expect(out).toStartWith('Title: My Script\nCredit: Written by\nAuthor: Jane Doe\n\n');
  });

  test('scene, cue, parenthetical, dialogue, action serialize with blank-line separation', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'INT. KITCHEN - DAY', type: 'scene' }),
        el({ text: 'JACK', type: 'character', character: 'JACK' }),
        el({ text: '(whispering)', type: 'parenthetical', character: 'JACK' }),
        el({ text: 'Hello there.', type: 'dialogue', character: 'JACK' }),
        el({ text: 'The door slams.', type: 'action' }),
      ]),
    );
    expect(out).toBe(
      'INT. KITCHEN - DAY\n\n@JACK\n(whispering)\nHello there.\n\nThe door slams.\n',
    );
  });

  test('page-number elements and non-readable elements are skipped', () => {
    const out = toFountain(
      screenplay([
        el({ text: '42.', type: 'page-number' }),
        el({ text: 'Blue Rev. (6/12/26)', type: 'page-number' }),
        el({ text: 'Some action.', type: 'action' }),
        el({ text: 'skip me', type: 'action', isReadable: false }),
      ]),
    );
    expect(out).toBe('Some action.\n');
  });

  test('title-page elements are excluded from body', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'MY TITLE', type: 'action', isTitlePage: true }),
        el({ text: 'INT. HOUSE - DAY', type: 'scene' }),
      ]),
    );
    expect(out).toBe('INT. HOUSE - DAY\n');
  });

  test('scene numbers serialize as #N#', () => {
    const out = toFountain(
      screenplay([el({ text: 'INT. LAB - NIGHT', type: 'scene', sceneNumber: '1A.' })]),
    );
    expect(out).toBe('INT. LAB - NIGHT #1A.#\n');
  });

  test('transitions are forced with >', () => {
    const out = toFountain(screenplay([el({ text: 'SMASH CUT TO:', type: 'transition' })]));
    expect(out).toBe('> SMASH CUT TO:\n');
  });

  test('action starting with . or > is forced with !', () => {
    const out = toFountain(
      screenplay([
        el({ text: '.45 on the table.', type: 'action' }),
        el({ text: '> ominous graffiti', type: 'action' }),
      ]),
    );
    expect(out).toBe('!.45 on the table.\n\n!> ominous graffiti\n');
  });

  test('mini-slugs serialize as forced sluglines', () => {
    const out = toFountain(screenplay([el({ text: 'LATER', type: 'mini-slug' })]));
    expect(out).toBe('.LATER\n');
  });

  test('a mini-slug Fountain cannot force stays an action line', () => {
    // A LEADING DOT is the one thing the dot-force can't carry: fountain-js
    // reads "..45 …" as action (its rule is `^\s*\.(?!\.+)`), so the marker
    // would just leak into the text. Nothing else is excluded.
    const out = toFountain(screenplay([el({ text: '.45 ON THE COUNTER', type: 'mini-slug' })]));
    expect(out).toBe('!.45 ON THE COUNTER\n');
  });

  test('other Fountain markers ride through the dot-force intact', () => {
    // ".#2 CAMERA" tokenizes as a scene heading whose text keeps the "#".
    const out = toFountain(screenplay([el({ text: '#2 CAMERA', type: 'mini-slug' })]));
    expect(out).toBe('.#2 CAMERA\n');
    const { tokens } = new Fountain().parse(out, true);
    expect(tokens.map((t) => [t.type, t.text])).toEqual([['scene_heading', '#2 CAMERA']]);
  });

  test("a mini-slug cuts the (CONT'D) chain — it is a cut in time or place", () => {
    const out = toFountain(
      screenplay([
        el({ text: 'MARGO', type: 'character', character: 'MARGO' }),
        el({ text: 'Go home.', type: 'dialogue', character: 'MARGO' }),
        el({ text: 'LATER', type: 'mini-slug' }),
        el({ text: 'MARGO', type: 'character', character: 'MARGO' }),
        el({ text: 'Still here.', type: 'dialogue', character: 'MARGO' }),
      ]),
    );
    expect(out).not.toContain("(CONT'D)");
  });

  test('a mini-slug never becomes a transition on re-parse', () => {
    // Plain "BACK TO:" matches fountain-js's transition rule (".+ TO:") and
    // used to render right-flush; the forced slugline pins it down.
    const { tokens } = new Fountain().parse(
      toFountain(screenplay([el({ text: 'BACK TO:', type: 'mini-slug' })])),
      true,
    );
    expect(tokens.filter((t) => t.type !== 'title_page').map((t) => t.type)).toEqual(['scene_heading']);
  });

  test('(MORE) parentheticals are dropped', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'JACK', type: 'character', character: 'JACK' }),
        el({ text: 'I was saying—', type: 'dialogue', character: 'JACK' }),
        el({ text: '(MORE)', type: 'parenthetical', character: 'JACK' }),
      ]),
    );
    expect(out).toBe('@JACK\nI was saying—\n');
  });

  test("(CONT'D) cue after page break merges into the same speech", () => {
    const out = toFountain(
      screenplay([
        el({ text: 'JACK', type: 'character', character: 'JACK', pageNum: 1 }),
        el({ text: 'First half of speech', type: 'dialogue', character: 'JACK', pageNum: 1 }),
        el({ text: '(MORE)', type: 'parenthetical', character: 'JACK', pageNum: 1 }),
        el({ text: '3.', type: 'page-number', pageNum: 2 }),
        el({ text: "JACK (CONT'D)", type: 'character', character: 'JACK', pageNum: 2 }),
        el({ text: 'and the second half.', type: 'dialogue', character: 'JACK', pageNum: 2 }),
      ]),
    );
    expect(out).toBe('@JACK\nFirst half of speech\nand the second half.\n');
  });

  test("(CONT'D) cue after intervening action keeps its own cue", () => {
    const out = toFountain(
      screenplay([
        el({ text: 'JACK', type: 'character', character: 'JACK' }),
        el({ text: 'Look at this.', type: 'dialogue', character: 'JACK' }),
        el({ text: 'He points at the wall.', type: 'action' }),
        el({ text: "JACK (CONT'D)", type: 'character', character: 'JACK' }),
        el({ text: 'See what I mean?', type: 'dialogue', character: 'JACK' }),
      ]),
    );
    expect(out).toBe(
      "@JACK\nLook at this.\n\nHe points at the wall.\n\n@JACK (CONT'D)\nSee what I mean?\n",
    );
  });

  test('cue with no following dialogue is demoted to action', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'ORPHAN CUE', type: 'character', character: 'ORPHAN CUE' }),
        el({ text: 'The lights go out.', type: 'action' }),
      ]),
    );
    expect(out).toBe('ORPHAN CUE\n\nThe lights go out.\n');
  });
});

describe('toFountain with images', () => {
  const img = (file: string, over: Partial<ScreenplayElement['img']> = {}): ScreenplayElement =>
    el({
      text: file,
      type: 'image',
      img: { file, widthPct: 71, fullPage: false, ...over },
    });
  const noteTokens = (out: string) =>
    new Fountain().parse(out, true).tokens.filter((t) => t.type === 'note');

  test('writes a standalone note with a blank line either side', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'INT. A - DAY', type: 'scene' }),
        img('p001-01.png'),
        el({ text: 'Someone waits.', type: 'action' }),
      ]),
    );
    expect(out).toBe('INT. A - DAY\n\n[[img: p001-01.png w=71]]\n\nSomeone waits.\n');
  });

  test('marks a full-page image', () => {
    const out = toFountain(
      screenplay([img('p001-01.png', { widthPct: 100, fullPage: true })]),
    );
    expect(out).toContain('[[img: p001-01.png w=100 full]]');
  });

  test('round-trips through fountain-js as its own note token', () => {
    // The whole encoding rests on this: a note GLUED to text folds into that
    // text's token instead, which is exactly what the fmt notes rely on. So
    // the separation is not cosmetic, and a string assertion cannot see it.
    const out = toFountain(
      screenplay([
        el({ text: 'INT. A - DAY', type: 'scene' }),
        img('p001-01.png'),
        el({ text: 'Someone waits.', type: 'action' }),
      ]),
    );
    const { tokens } = new Fountain().parse(out, true);
    expect(tokens.map((t) => t.type)).toEqual(['scene_heading', 'note', 'action']);
    // Brackets already stripped by the tokenizer — which is why parseImgNote
    // takes the INNER text and, unlike fmtClasses, matches no brackets.
    expect(tokens[1].text).toBe('img: p001-01.png w=71');
  });

  test('what the serializer writes is what the reader parses', () => {
    // Both directions in one assertion. Either half can be changed alone and
    // stay green in its own suite; the pair cannot.
    const out = toFountain(
      screenplay([img('p012-03.png', { widthPct: 43, fullPage: false }),
                  img('p012-04.png', { widthPct: 100, fullPage: true })]),
    );
    expect(noteTokens(out).map((t) => parseImgNote(t.text!))).toEqual([
      { file: 'p012-03.png', widthPct: 43, fullPage: false },
      { file: 'p012-04.png', widthPct: 100, fullPage: true },
    ]);
  });

  test('an image on a detected title page still reaches the body', () => {
    // isBody() drops title-page elements. The designed title card is the one
    // exception, and it is also the most valuable image in the document.
    const out = toFountain(
      screenplay([
        el({ text: 'MY TITLE', type: 'action', isTitlePage: true }),
        { ...img('p001-01.png'), isTitlePage: true },
        el({ text: 'INT. HOUSE - DAY', type: 'scene' }),
      ]),
    );
    expect(out).toBe('[[img: p001-01.png w=71]]\n\nINT. HOUSE - DAY\n');
  });

  test('an unreadable image is still dropped', () => {
    // The title-page exemption is not an exemption from everything: whatever
    // marked an element unreadable was not looking at its `isTitlePage` flag.
    const out = toFountain(
      screenplay([
        { ...img('p001-01.png'), isReadable: false },
        el({ text: 'INT. HOUSE - DAY', type: 'scene' }),
      ]),
    );
    expect(out).toBe('INT. HOUSE - DAY\n');
  });

  test('two images in a row are two notes, not one paragraph', () => {
    const out = toFountain(
      screenplay([img('p001-01.png'), img('p001-02.png')]),
    );
    expect(out).toBe('[[img: p001-01.png w=71]]\n\n[[img: p001-02.png w=71]]\n');
    expect(noteTokens(out).map((t) => t.text)).toEqual([
      'img: p001-01.png w=71', 'img: p001-02.png w=71',
    ]);
  });

  test('an image as the only element of the document', () => {
    // The degenerate ends of the stream: nothing before it and nothing after.
    expect(toFountain(screenplay([img('p001-01.png')])))
      .toBe('[[img: p001-01.png w=71]]\n');
  });

  test('an image element with no img is emitted as nothing at all', () => {
    // Unreachable through parseLines, which sets `img` on every image element
    // it mints — so this pins the fallback rather than a path. The wrong
    // answer is loud: the `default:` branch would print the bare filename as
    // an action line, which is what this case used to do.
    const out = toFountain(
      screenplay([
        el({ text: 'INT. A - DAY', type: 'scene' }),
        el({ text: 'p001-01.png', type: 'image' }),
        el({ text: 'Someone waits.', type: 'action' }),
      ]),
    );
    expect(out).toBe('INT. A - DAY\n\nSomeone waits.\n');
  });

  test('is byte-stable under a settings flip, like the fmt notes', () => {
    // The .fountain is the cache boundary: toFountain output must not move
    // when a rendering option changes, or the app re-renders a "new" book
    // from a cached parse. `preserveFontShifts` gates RENDERING, so it may not
    // move a byte here. No option gates pictures yet; when one does, its flip
    // belongs in this test beside this one, for the same reason.
    const doc = () =>
      screenplay([
        el({ text: 'INT. A - DAY', type: 'scene' }),
        img('p001-01.png'),
        el({ text: 'Someone waits.', type: 'action', fmt: { family: 'sans' } }),
      ]);
    const canonical = toFountain(doc(), undefined, DEFAULT_FORMAT_OPTIONS);
    // Pin the note's presence first: without this, the equality below would
    // also hold in the failure mode it exists to catch — a serializer that
    // dropped the note under BOTH settings.
    expect(canonical).toContain('[[img: p001-01.png w=71]]');
    expect(toFountain(doc(), undefined, { ...DEFAULT_FORMAT_OPTIONS, preserveFontShifts: false }))
      .toBe(canonical);
  });

  // ── an image that interrupts a speech ──────────────────

  test('an image between a cue and its speech does not destroy the speech', () => {
    // `prepare` demotes a cue whose next element is neither dialogue nor
    // parenthetical. An image is not evidence of that, so the lookahead skips
    // it — and the note is then held until the speech closes, because a cue
    // alone in its own paragraph is not a cue to fountain-js either: it
    // re-parses as action with the "@" showing as literal text.
    const out = toFountain(
      screenplay([
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY' }),
        img('p001-01.png'),
        el({ text: 'Her line.', type: 'dialogue', character: 'BUNNY' }),
      ]),
    );
    expect(out).toBe('@BUNNY\nHer line.\n\n[[img: p001-01.png w=71]]\n');
    const { tokens } = new Fountain().parse(out, true);
    expect(tokens.map((t) => t.type)).toEqual([
      'dialogue_begin', 'character', 'dialogue', 'dialogue_end', 'note',
    ]);
  });

  test('an image inside a speech is deferred past it, in order', () => {
    // Two of them, so "deferred" cannot be "kept the last one".
    const out = toFountain(
      screenplay([
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY' }),
        el({ text: 'First half.', type: 'dialogue', character: 'BUNNY' }),
        img('p001-01.png'),
        el({ text: 'Second half.', type: 'dialogue', character: 'BUNNY' }),
        img('p001-02.png'),
        el({ text: 'INT. NEXT - DAY', type: 'scene' }),
      ]),
    );
    expect(out).toBe(
      '@BUNNY\nFirst half.\nSecond half.\n\n' +
        '[[img: p001-01.png w=71]]\n\n[[img: p001-02.png w=71]]\n\nINT. NEXT - DAY\n',
    );
  });

  test('a deferred image is never dropped when the speech ends the document', () => {
    // The flush has to happen in the final closeBlock too, or a picture on
    // the last page of a script that ends on dialogue vanishes.
    const out = toFountain(
      screenplay([
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY' }),
        el({ text: 'Her line.', type: 'dialogue', character: 'BUNNY' }),
        img('p009-01.png'),
      ]),
    );
    expect(out).toBe('@BUNNY\nHer line.\n\n[[img: p009-01.png w=71]]\n');
  });

  test('with no speech open the picture is written where it stands', () => {
    // "Why not just always defer?" — because a stray dialogue element (one
    // whose cue the parser never found) is written straight to the output
    // with no block ever opening, and a note held past it comes out on the
    // wrong side of it. Deferral is for an OPEN speech and nothing else.
    const out = toFountain(
      screenplay([
        img('p001-01.png'),
        el({ text: 'An orphan line with no cue.', type: 'dialogue' }),
      ]),
    );
    expect(out).toBe('[[img: p001-01.png w=71]]\n\nAn orphan line with no cue.\n');
  });

  test('a cue with genuinely nothing under it is still demoted', () => {
    // The lookahead skips images; it must not skip everything. An image is
    // not dialogue, and a cue followed by ONLY an image has no speech.
    const out = toFountain(
      screenplay([
        el({ text: 'ORPHAN CUE', type: 'character', character: 'ORPHAN CUE' }),
        img('p001-01.png'),
        el({ text: 'The lights go out.', type: 'action' }),
      ]),
    );
    expect(out).toBe(
      'ORPHAN CUE\n\n[[img: p001-01.png w=71]]\n\nThe lights go out.\n',
    );
  });

  test('the lookahead skips images and ONLY images', () => {
    // A cue, then action, then somebody else's speech further down. "Skip
    // until you find dialogue" reaches that speech and keeps the cue, and a
    // cue alone in its paragraph re-parses as action with the "@" showing. A
    // mutation sweep found that version and nothing else in the suite saw it,
    // because every other orphan cue in here has nothing after it at all.
    const out = toFountain(
      screenplay([
        el({ text: 'ORPHAN CUE', type: 'character', character: 'ORPHAN CUE' }),
        el({ text: 'The lights go out.', type: 'action' }),
        el({ text: 'NOOR', type: 'character', character: 'NOOR' }),
        el({ text: 'Her line.', type: 'dialogue', character: 'NOOR' }),
      ]),
    );
    expect(out).toBe('ORPHAN CUE\n\nThe lights go out.\n\n@NOOR\nHer line.\n');
    expect(out).not.toContain('@ORPHAN CUE');
  });

  test('an image between a cue and a parenthetical keeps the cue', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY' }),
        img('p001-01.png'),
        el({ text: '(quietly)', type: 'parenthetical', character: 'BUNNY' }),
        el({ text: 'Her line.', type: 'dialogue', character: 'BUNNY' }),
      ]),
    );
    expect(out).toBe('@BUNNY\n(quietly)\nHer line.\n\n[[img: p001-01.png w=71]]\n');
  });

  test('a page marker never separates a deferred image from its own page', () => {
    // The page-marker branch calls closeBlock() itself, and the flush lives
    // inside closeBlock — so a picture from page 1 comes out on page 1's side
    // of the "= pg 2" marker, attached to the speech it interrupted.
    const out = toFountain(
      screenplay([
        el({ text: '1.', type: 'page-number', pageNum: 1 }),
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY', pageNum: 1 }),
        el({ text: 'Her line.', type: 'dialogue', character: 'BUNNY', pageNum: 1 }),
        { ...img('p001-01.png'), pageNum: 1 },
        el({ text: 'INT. NEXT - DAY', type: 'scene', pageNum: 2 }),
      ]),
      undefined,
      { ...DEFAULT_FORMAT_OPTIONS, showPageMarkers: true },
    );
    expect(out).toBe(
      '@BUNNY\nHer line.\n\n[[img: p001-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n',
    );
  });

  test('an image never anchors a page marker', () => {
    // A marker is a navigation anchor onto a line of text, and a picture is
    // not one — so a page that OPENS on a plate defers its marker to the next
    // real element, exactly as a page opening on the tail of a speech does.
    // Nothing is deferred here (no block is open), so this is the rule stated
    // on its own, away from the speech-corruption case below that forces it.
    const out = toFountain(
      screenplay([
        el({ text: '1.', type: 'page-number', pageNum: 1 }),
        el({ text: 'Someone waits.', type: 'action', pageNum: 1 }),
        { ...img('p002-01.png'), pageNum: 2 },
        el({ text: 'INT. NEXT - DAY', type: 'scene', pageNum: 2 }),
      ]),
      undefined,
      { ...DEFAULT_FORMAT_OPTIONS, showPageMarkers: true },
    );
    expect(out).toBe(
      'Someone waits.\n\n[[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n',
    );
  });

  test('a sheet holding nothing but a plate gets no marker of its own', () => {
    // The deliberate consequence of the rule above, named so it is a decision
    // and not a surprise: page 2 is a full-page plate with no text on it, so
    // the next marker the reader sees is page 3's.
    const out = toFountain(
      screenplay([
        el({ text: '1.', type: 'page-number', pageNum: 1 }),
        el({ text: 'Someone waits.', type: 'action', pageNum: 1 }),
        { ...img('p002-01.png', { widthPct: 100, fullPage: true }), pageNum: 2 },
        el({ text: 'INT. NEXT - DAY', type: 'scene', pageNum: 3 }),
      ]),
      undefined,
      { ...DEFAULT_FORMAT_OPTIONS, showPageMarkers: true },
    );
    expect(out).toBe(
      'Someone waits.\n\n[[img: p002-01.png w=100 full]]\n\n= pg 3\n\nINT. NEXT - DAY\n',
    );
    expect(out).not.toContain('= pg 2');
  });

  test('markers on: a plate at the top of a page does not break the speech under it', () => {
    // The page-marker branch runs BEFORE the switch and calls closeBlock()
    // itself, so an image left out of its guard closes an open speech before
    // `case 'image'` can defer anything — and the defect the deferral exists
    // to prevent comes straight back, but only with showPageMarkers on.
    // A cue at the foot of page 1, the plate at the top of page 2, the speech
    // under it. This used to emit "@BUNNY" as its own paragraph, which
    // fountain-js re-parses as action with the "@" shown to the reader.
    const els = () => [
      el({ text: 'BUNNY', type: 'character', character: 'BUNNY', pageNum: 1 }),
      { ...img('p002-01.png'), pageNum: 2 },
      el({ text: 'Her line.', type: 'dialogue', character: 'BUNNY', pageNum: 2 }),
      el({ text: 'INT. NEXT - DAY', type: 'scene', pageNum: 2 }),
    ];
    const out = toFountain(screenplay(els()), undefined,
      { ...DEFAULT_FORMAT_OPTIONS, showPageMarkers: true });
    expect(out).toBe(
      '@BUNNY\nHer line.\n\n[[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n',
    );
    const { tokens } = new Fountain().parse(out, true);
    expect(tokens.map((t) => t.type)).toEqual([
      'dialogue_begin', 'character', 'dialogue', 'dialogue_end',
      'note', 'synopsis', 'scene_heading',
    ]);
    // And the marker is the ONLY difference the toggle makes, which is the
    // property that says the two knobs are independent. Off is spelled out:
    // markers default ON, so the defaults are the "on" half, not the "off".
    const off = toFountain(screenplay(els()), undefined,
      { ...DEFAULT_FORMAT_OPTIONS, showPageMarkers: false });
    expect(off).toBe(out.replace('\n\n= pg 2', ''));
  });

  test("a plate above a (CONT'D) cue does not switch the rejoin off", () => {
    // The backward half of "a picture is not evidence about its neighbours".
    // prepare's rejoin loop asks whether the PREVIOUS element is this
    // speaker's dialogue; a plate at the top of the sheet stood in that slot
    // and answered no, so the repeated cue survived and one speech came out
    // as two blocks with a duplicated "BUNNY (CONT'D)". The reader saw
    // rejoinSplitDialogue silently switched off by a picture.
    const out = toFountain(
      screenplay([
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY', pageNum: 1 }),
        el({ text: 'First half.', type: 'dialogue', character: 'BUNNY', pageNum: 1 }),
        { ...img('p002-01.png'), pageNum: 2 },
        el({ text: "BUNNY (CONT'D)", type: 'character', character: 'BUNNY', pageNum: 2 }),
        el({ text: 'second half.', type: 'dialogue', character: 'BUNNY', pageNum: 2 }),
      ]),
    );
    expect(out).toBe(
      '@BUNNY\nFirst half.\nsecond half.\n\n[[img: p002-01.png w=71]]\n',
    );
    expect(out).not.toContain("(CONT'D)");
  });

  test('and the rejoin is still a toggle, not a thing images turned on', () => {
    // The other half of that assertion: with rejoinSplitDialogue off the
    // repeated cue must come back, or the fix above would have hard-coded a
    // behaviour the option is supposed to control. Markers pinned off, so the
    // one knob under test is the only one that moved: they default ON, and
    // the restored cue would anchor "= pg 2", which this test is not about.
    const out = toFountain(
      screenplay([
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY', pageNum: 1 }),
        el({ text: 'First half.', type: 'dialogue', character: 'BUNNY', pageNum: 1 }),
        { ...img('p002-01.png'), pageNum: 2 },
        el({ text: "BUNNY (CONT'D)", type: 'character', character: 'BUNNY', pageNum: 2 }),
        el({ text: 'second half.', type: 'dialogue', character: 'BUNNY', pageNum: 2 }),
      ]),
      undefined,
      { ...DEFAULT_FORMAT_OPTIONS, rejoinSplitDialogue: false, showPageMarkers: false },
    );
    expect(out).toBe(
      "@BUNNY\nFirst half.\n\n[[img: p002-01.png w=71]]\n\n@BUNNY (CONT'D)\nsecond half.\n",
    );
  });

  test('the emitted width is an integer even if the element carries a fraction', () => {
    // WIDTH_WORD reads integers only, so a fractional width parses back as the
    // default 100 and every picture renders full-column with nothing red.
    // Today columnWidthPct rounds, which is why this can only be reached by
    // hand — and is exactly why the round is repeated at the emit site rather
    // than trusted from a module away.
    const out = toFountain(screenplay([img('p001-01.png', { widthPct: 71.4 })]));
    expect(out).toBe('[[img: p001-01.png w=71]]\n');
    expect(parseImgNote(noteTokens(out)[0].text!)?.widthPct).toBe(71);
  });

  test('markers on: a parenthetical at the top of a page keeps its speech too', () => {
    // No image in this one. The guard's OTHER two clauses were uncovered for
    // the same shape — a mutation sweep removing `!== 'parenthetical'`
    // survived the whole suite — and the failure mode is the one this task
    // just fixed for images: the marker branch closes the block, the
    // continuation lands with no cue, and the speech breaks in half. Kept
    // beside the image cases because it is one guard and one rule.
    const out = toFountain(
      screenplay([
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY', pageNum: 1 }),
        el({ text: 'First half.', type: 'dialogue', character: 'BUNNY', pageNum: 1 }),
        el({ text: '(quietly)', type: 'parenthetical', character: 'BUNNY', pageNum: 2 }),
        el({ text: 'second half.', type: 'dialogue', character: 'BUNNY', pageNum: 2 }),
        el({ text: 'INT. NEXT - DAY', type: 'scene', pageNum: 2 }),
      ]),
      undefined,
      { ...DEFAULT_FORMAT_OPTIONS, showPageMarkers: true },
    );
    expect(out).toBe(
      '@BUNNY\nFirst half.\n(quietly)\nsecond half.\n\n= pg 2\n\nINT. NEXT - DAY\n',
    );
  });

  test('markers on: a plate in a page-break seam does not re-split the speech', () => {
    // The second reachable shape, same root cause. rejoinSplitDialogue drops
    // a (CONT'D) cue whose speech continues on the next page, leaving
    // character | dialogue | image | dialogue — so the block is open when the
    // plate arrives and the continuation lands with no cue above it. It used
    // to come out as a loose action paragraph.
    const out = toFountain(
      screenplay([
        el({ text: 'BUNNY', type: 'character', character: 'BUNNY', pageNum: 1 }),
        el({ text: 'First half.', type: 'dialogue', character: 'BUNNY', pageNum: 1 }),
        { ...img('p002-01.png'), pageNum: 2 },
        el({ text: 'second half.', type: 'dialogue', character: 'BUNNY', pageNum: 2 }),
        el({ text: 'INT. NEXT - DAY', type: 'scene', pageNum: 2 }),
      ]),
      undefined,
      { ...DEFAULT_FORMAT_OPTIONS, showPageMarkers: true },
    );
    expect(out).toBe(
      '@BUNNY\nFirst half.\nsecond half.\n\n[[img: p002-01.png w=71]]\n\n' +
        '= pg 2\n\nINT. NEXT - DAY\n',
    );
    const { tokens } = new Fountain().parse(out, true);
    expect(tokens.filter((t) => t.type === 'action')).toEqual([]);
  });
});

// ── round-trip through fountain-js ───────────────────────

describe('fountain-js round-trip', () => {
  test('serialized output tokenizes back to the same structure', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'INT. KITCHEN - DAY', type: 'scene', sceneNumber: '7' }),
        el({ text: 'Jack enters.', type: 'action' }),
        el({ text: 'JACK (V.O.)', type: 'character', character: 'JACK' }),
        el({ text: '(tired)', type: 'parenthetical', character: 'JACK' }),
        el({ text: 'Long day.', type: 'dialogue', character: 'JACK' }),
        el({ text: 'FADE OUT.', type: 'transition' }),
      ]),
      { title: 'Round Trip', author: 'Tester' },
    );
    const { tokens } = new Fountain().parse(out, true);
    const types = tokens.map((t) => t.type);
    expect(types).toEqual([
      'title', 'credit', 'author',
      'scene_heading', 'action',
      'dialogue_begin', 'character', 'parenthetical', 'dialogue', 'dialogue_end',
      'transition',
    ]);
    const scene = tokens.find((t) => t.type === 'scene_heading')!;
    expect(scene.scene_number).toBe('7');
    const cue = tokens.find((t) => t.type === 'character')!;
    expect(cue.text).toBe('JACK (V.O.)');
  });

  test('a parsed mini-slug reaches the EPUB as a mini-slug', () => {
    // The whole seam: parser element → Fountain → tokens → XHTML. The class
    // the stylesheet styles has to be the one the renderer emits.
    const out = toFountain(
      screenplay([
        el({ text: 'INT. KITCHEN - DAY', type: 'scene' }),
        el({ text: 'Jack waits.', type: 'action' }),
        el({ text: 'LATER', type: 'mini-slug' }),
        el({ text: 'Jack is still waiting.', type: 'action' }),
      ]),
    );
    const { tokens } = new Fountain().parse(out, true);
    const body = tokensToBody(tokens);
    expect(body.files[0].xhtml).toContain('<p class="mini-slug">LATER</p>');
  });
});

// ── regression: real-PDF artifacts ───────────────────────

describe('PDF artifact handling', () => {
  test('(MORE) is dropped regardless of classified type', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'JACK', type: 'character', character: 'JACK' }),
        el({ text: 'Speech before break', type: 'dialogue', character: 'JACK' }),
        el({ text: '(MORE)', type: 'action' }),
        el({ text: "JACK (CONT'D)", type: 'character', character: 'JACK' }),
        el({ text: 'speech after break.', type: 'dialogue', character: 'JACK' }),
      ]),
    );
    expect(out).toBe('@JACK\nSpeech before break\nspeech after break.\n');
  });

  test("curly-apostrophe (CONT’D) cue merges across page break", () => {
    const out = toFountain(
      screenplay([
        el({ text: 'DEV', type: 'character', character: 'DEV' }),
        el({ text: 'First part', type: 'dialogue', character: 'DEV' }),
        el({ text: 'DEV (CONT’D)', type: 'character', character: 'DEV' }),
        el({ text: 'second part.', type: 'dialogue', character: 'DEV' }),
      ]),
    );
    expect(out).toBe('@DEV\nFirst part\nsecond part.\n');
  });

  test('cue internal whitespace is normalized', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'MIDDLE SEAT  PASSENGER  (V.O.)', type: 'character', character: 'MIDDLE SEAT PASSENGER' }),
        el({ text: 'Hm.', type: 'dialogue', character: 'MIDDLE SEAT PASSENGER' }),
      ]),
    );
    expect(out).toBe('@MIDDLE SEAT PASSENGER (V.O.)\nHm.\n');
  });
});

// ── inline emphasis pass-through ─────────────────────────

describe('styled text emission', () => {
  test('dialogue and action emit their styled variant', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'The sign reads DANGER.', type: 'action', styledText: 'The sign reads **DANGER**.' }),
        el({ text: 'JACK', type: 'character', character: 'JACK' }),
        el({ text: 'I want insurance now.', type: 'dialogue', character: 'JACK', styledText: 'I want *insurance* now.' }),
      ]),
    );
    expect(out).toContain('The sign reads **DANGER**.');
    expect(out).toContain('I want *insurance* now.');
  });

  test('parentheticals and cues stay plain (markers would break recognition)', () => {
    const out = toFountain(
      screenplay([
        el({ text: 'JACK', type: 'character', character: 'JACK', styledText: '**JACK**' }),
        el({ text: '(scream singing)', type: 'parenthetical', character: 'JACK', styledText: '*(scream singing)*' }),
        el({ text: 'La la la', type: 'dialogue', character: 'JACK' }),
      ]),
    );
    expect(out).toContain('@JACK\n(scream singing)\nLa la la');
  });
});

describe('font-shift notes', () => {
  test('an action note is glued directly above its block, no blank line', () => {
    const out = toFountain(screenplay([
      el({ text: 'INT. NEWSROOM - DAY', type: 'scene' }),
      el({ text: 'CHYRON: BREAKING.', type: 'action', fmt: { family: 'sans' } }),
    ]));
    expect(out).toContain('[[fmt: sans]]\nCHYRON: BREAKING.');
  });

  test('a dialogue note leads the line, space separated', () => {
    const out = toFountain(screenplay([
      el({ text: 'WREN', type: 'character', character: 'WREN' }),
      el({ text: 'Read it back.', type: 'dialogue', character: 'WREN', fmt: { family: 'sans' } }),
    ]));
    expect(out).toContain('@WREN\n[[fmt: sans]] Read it back.');
  });

  test('family and size ride one note, family first', () => {
    const out = toFountain(screenplay([
      el({ text: 'A SIGN.', type: 'action', fmt: { family: 'sans', size: '+2' } }),
    ]));
    expect(out).toContain('[[fmt: sans +2]]\nA SIGN.');
  });

  test('a size-only shift emits the size alone', () => {
    const out = toFountain(screenplay([
      el({ text: 'INSERT: THE CARD.', type: 'action', fmt: { size: '+1' } }),
    ]));
    expect(out).toContain('[[fmt: +1]]\nINSERT: THE CARD.');
  });

  test('an element with no fmt emits no note', () => {
    expect(toFountain(screenplay([el({ text: 'Plain action.', type: 'action' })]))).not.toContain('[[');
  });

  test('only action and dialogue carry notes', () => {
    // Markers in a cue, parenthetical or slug break recognition — the same
    // rule styledText already lives under (registry 9d).
    const out = toFountain(screenplay([
      el({ text: 'INT. A - DAY', type: 'scene', fmt: { family: 'sans' } }),
      el({ text: 'WREN', type: 'character', character: 'WREN', fmt: { family: 'sans' } }),
      el({ text: '(beat)', type: 'parenthetical', fmt: { family: 'sans' } }),
      el({ text: 'Hello.', type: 'dialogue', character: 'WREN' }),
      el({ text: 'CUT TO:', type: 'transition', fmt: { size: '+1' } }),
      el({ text: 'LATER', type: 'mini-slug', fmt: { size: '+1' } }),
    ]));
    expect(out).not.toContain('[[');
  });

  test('the note survives the forced-action prefix', () => {
    // "!" forcing must stay at the head of the TEXT, not of the note.
    const out = toFountain(screenplay([
      el({ text: '.45 ON THE TABLE', type: 'action', fmt: { size: '+1' } }),
    ]));
    expect(out).toContain('[[fmt: +1]]\n!.45 ON THE TABLE');
  });
});

// ── boneyard delimiters must never be emitted ────────────
//
// Fountain deletes everything between `/*` and `*/`. Neither is ever
// written on purpose: the parser stamps one emphasis run per source line,
// so a lyric's `/` separator landing at a run boundary puts a `*` hard
// against a `/`. A real 110-page script lost 63% of itself this way, in
// silence, because stage 1 was perfectly correct and stage 2 ate the file.
//
// Note a naive fix of inserting a space does NOT work: Fountain requires
// no whitespace inside the delimiters, so `*text *` stops italicizing and
// renders literal asterisks. The slash has to move OUT of the run.

describe('boneyard delimiters', () => {
  const bothRuns = () => screenplay([
    el({ text: 'SINGER', type: 'character', character: 'SINGER' }),
    el({
      text: 'Roll the tape, we ride tonight / Neon on the overpass',
      styledText: '*Roll the tape, we ride tonight /* *Neon on the overpass*',
      type: 'dialogue',
    }),
    el({ text: 'INT. DINER - NIGHT', type: 'scene', pageNum: 2 }),
    el({ text: 'A jukebox coughs to life.', type: 'action', pageNum: 2 }),
    el({ text: 'SINGER', type: 'character', character: 'SINGER', pageNum: 3 }),
    el({
      text: '/ And the morning takes it back',
      styledText: '*/ And the morning takes it back*',
      type: 'dialogue',
      pageNum: 3,
    }),
    el({ text: 'INT. GARAGE - DAWN', type: 'scene', pageNum: 4 }),
    el({ text: 'The last chord rings out.', type: 'action', pageNum: 4 }),
  ]);

  test('an emphasis run ending on a slash does not emit /*', () => {
    expect(toFountain(bothRuns())).not.toContain('/*');
  });

  test('an emphasis run starting on a slash does not emit */', () => {
    expect(toFountain(bothRuns())).not.toContain('*/');
  });

  test('nothing between the two lyrics is swallowed by fountain-js', () => {
    // The bug in one assertion: everything between the delimiters vanished.
    const tokens = new Fountain().parse(toFountain(bothRuns()), true).tokens ?? [];
    const text = tokens.map((t) => t.text ?? '').join('\n');
    expect(text).toContain('INT. DINER - NIGHT');
    expect(text).toContain('A jukebox coughs to life.');
    expect(text).toContain('INT. GARAGE - DAWN');
    expect(text).toContain('The last chord rings out.');
  });

  test('the slash survives, and the words around it keep their emphasis', () => {
    const out = toFountain(bothRuns());
    expect(out).toContain('/');
    expect(out).toContain('*Roll the tape, we ride tonight*');
    expect(out).toContain('*And the morning takes it back*');
  });
});
