import { describe, test, expect } from 'bun:test';
import {
  fmtClasses, fmtSizeStep, imageNoteFiles, isSafeImageFile, parseImgNote, rendersImage, stripNotes,
} from '../src/fountain/notes';

describe('stripNotes', () => {
  test('a leading note and the newline it sat on both go', () => {
    expect(stripNotes('[[fmt: sans]]\nCHYRON: LIVE.')).toBe('CHYRON: LIVE.');
  });

  test('an inline note and its extra space go', () => {
    expect(stripNotes('[[fmt: sans]] Read it back.')).toBe('Read it back.');
  });

  test('a note in the middle of a line goes', () => {
    // Pre-existing wart this fixes: any hand-written note used to render as
    // literal text, which the Fountain spec says is wrong.
    expect(stripNotes('He leaves [[check this]] quickly.')).toBe('He leaves quickly.');
  });

  test('newlines inside a multi-line token survive', () => {
    // A lyrics/verse dialogue token carries its own line breaks.
    expect(stripNotes('[[fmt: +1]] one\ntwo\nthree')).toBe('one\ntwo\nthree');
  });

  test('text with no notes is returned untouched, character for character', () => {
    const s = '  double  spaced  and\n  indented  ';
    expect(stripNotes(s)).toBe(s);
  });

  test('an unterminated bracket is left alone rather than eating the line', () => {
    expect(stripNotes('He said [[ and stopped')).toBe('He said [[ and stopped');
  });
});

describe('fmtClasses', () => {
  test('a family note yields its class', () => {
    expect(fmtClasses('[[fmt: sans]]\nX')).toBe(' fmt-sans');
  });

  test('all four families and all three sizes map', () => {
    for (const [word, cls] of [['mono', 'fmt-mono'], ['serif', 'fmt-serif'],
                               ['sans', 'fmt-sans'], ['cursive', 'fmt-cursive'],
                               ['-1', 'fmt-minus1'], ['+1', 'fmt-plus1'],
                               ['+2', 'fmt-plus2']]) {
      expect(fmtClasses(`[[fmt: ${word}]]\nX`), word).toBe(` ${cls}`);
    }
  });

  test('family and size together yield both classes', () => {
    expect(fmtClasses('[[fmt: sans +2]]\nX')).toBe(' fmt-sans fmt-plus2');
  });

  test('only a LEADING note counts', () => {
    expect(fmtClasses('Text first [[fmt: sans]]')).toBe('');
  });

  test('a non-fmt note yields no classes', () => {
    expect(fmtClasses('[[a production note]]\nX')).toBe('');
  });

  test('malformed content is ignored word by word, never thrown', () => {
    expect(fmtClasses('[[fmt: teal +9 sans]]\nX')).toBe(' fmt-sans');
    expect(fmtClasses('[[fmt:]]\nX')).toBe('');
    expect(fmtClasses('[[fmt: ]]\nX')).toBe('');
  });
});

describe('fmtSizeStep', () => {
  test('reads the size word out of a leading note', () => {
    expect(fmtSizeStep('[[fmt: sans +2]]\nX')).toBe('+2');
    expect(fmtSizeStep('[[fmt: -1]]\nX')).toBe('-1');
  });

  test('a family-only note has no size step', () => {
    expect(fmtSizeStep('[[fmt: sans]]\nX')).toBeUndefined();
  });

  test('no note at all has no size step', () => {
    expect(fmtSizeStep('Plain action.')).toBeUndefined();
  });
});

describe('parseImgNote', () => {
  // The INNER text of a standalone note, brackets already stripped — that is
  // what fountain-js puts on a `note` token's `text` (pinned in
  // serialize.test.ts, "round-trips through fountain-js as its own note
  // token"). Unlike fmtClasses, which reads a leading note still embedded in
  // an action or dialogue token's text and so has to match the brackets too.

  test('reads a filename and a width', () => {
    expect(parseImgNote('img: p012-01.png w=71')).toEqual({
      file: 'p012-01.png', widthPct: 71, fullPage: false,
    });
  });

  test('reads the full-page flag', () => {
    expect(parseImgNote('img: p001-01.png w=100 full')?.fullPage).toBe(true);
  });

  test('tolerates a missing width, like the fmt notes tolerate junk', () => {
    expect(parseImgNote('img: p003-02.png')).toEqual({
      file: 'p003-02.png', widthPct: 100, fullPage: false,
    });
  });

  test('ignores words it does not know', () => {
    expect(parseImgNote('img: a.png w=40 sideways lolcat')).toEqual({
      file: 'a.png', widthPct: 40, fullPage: false,
    });
  });

  test('the words may come in any order', () => {
    expect(parseImgNote('img: a.png full w=71')).toEqual({
      file: 'a.png', widthPct: 71, fullPage: true,
    });
  });

  test('a repeated width: the last one wins', () => {
    // Not a case we emit — pinned because "first wins" and "last wins" are
    // both one-character changes and neither is obviously right from the code.
    expect(parseImgNote('img: a.png w=20 w=80')?.widthPct).toBe(80);
  });

  test('is not a note of another kind', () => {
    expect(parseImgNote('fmt: sans +1')).toBeUndefined();
    expect(parseImgNote('a normal writer note')).toBeUndefined();
  });

  test('the keyword is case-SENSITIVE, exactly like fmt:', () => {
    // We emit `img: ` and nothing else, and LEADING_FMT above has no /i
    // either, so an unrecognized note simply stays invisible (stripNotes
    // removes every note regardless of what is in it). A case-insensitive
    // keyword would also be a half-measure: the allowlist below is what
    // decides the FILENAME, and it wants a lowercase `.png`.
    expect(parseImgNote('IMG: p001-01.png w=71')).toBeUndefined();
    expect(parseImgNote('Img: p001-01.png')).toBeUndefined();
  });

  test('an uppercase extension is not a png as far as this is concerned', () => {
    // Same reasoning from the other side: the name is used VERBATIM as a zip
    // entry and as a filename read off disk, and a case-sensitive filesystem
    // does not have `p001-01.PNG` when the picture folder holds `p001-01.png`.
    expect(parseImgNote('img: p001-01.PNG')).toBeUndefined();
  });

  test('a note that is only the keyword yields nothing', () => {
    expect(parseImgNote('img:')).toBeUndefined();
    expect(parseImgNote('img: ')).toBeUndefined();
    // `w=71` is the first non-space run, so it is read as the FILENAME and
    // fails the allowlist. Recorded rather than special-cased: every way out
    // of a malformed note is the same one.
    expect(parseImgNote('img:   w=71')).toBeUndefined();
  });

  // The .fountain is a text file a user can hand-edit, and this filename
  // becomes a path inside a zip. These are the cases that matter.
  test('refuses path traversal', () => {
    expect(parseImgNote('img: ../../etc/passwd.png')).toBeUndefined();
    // Rejected by the allowlist's character class (`%` and `/` are not in it),
    // NOT by any rule that looks for `..` — there is no such rule. Worth
    // saying, because the allowlist is the whole defence: a name of nothing
    // but dots, like `..png`, is accepted and is harmless precisely because
    // no separator can appear beside it.
    expect(parseImgNote('img: ..%2f..%2fx.png')).toBeUndefined();
    expect(parseImgNote('img: ..png')).toEqual({
      file: '..png', widthPct: 100, fullPage: false,
    });
  });

  test('refuses separators of both kinds and absolute paths', () => {
    expect(parseImgNote('img: sub/dir.png')).toBeUndefined();
    expect(parseImgNote('img: sub\\dir.png')).toBeUndefined();
    expect(parseImgNote('img: /tmp/x.png')).toBeUndefined();
  });

  test('a name that only STARTS like a picture is refused: the allowlist is anchored at the end', () => {
    // The `.png` has to be where the name stops. Every case above fails on a
    // character before the extension; these are clean up to `a.png` and carry
    // the payload after it, so only the closing anchor refuses them.
    expect(parseImgNote('img: a.png/../../x')).toBeUndefined();
    expect(parseImgNote('img: a.png.exe')).toBeUndefined();
    expect(parseImgNote('img: a.png\\..\\x')).toBeUndefined();
    expect(parseImgNote('img: a.png%2f..%2fx')).toBeUndefined();
  });

  test('refuses anything that is not a .png', () => {
    expect(parseImgNote('img: script.fountain')).toBeUndefined();
    expect(parseImgNote('img: payload.svg')).toBeUndefined();
    expect(parseImgNote('img: .png')).toBeUndefined(); // no stem at all
  });

  test('refuses an absurdly long name', () => {
    expect(parseImgNote(`img: ${'a'.repeat(200)}.png`)).toBeUndefined();
  });

  test('the length bound is on the stem, and it is 64', () => {
    // Stated because the regex reads like a limit on the whole filename and
    // is not: `{1,64}` counts what precedes `.png`.
    expect(parseImgNote(`img: ${'a'.repeat(64)}.png`)?.file).toBe(`${'a'.repeat(64)}.png`);
    expect(parseImgNote(`img: ${'a'.repeat(65)}.png`)).toBeUndefined();
  });

  test('clamps a nonsense width rather than trusting it', () => {
    expect(parseImgNote('img: a.png w=9999')?.widthPct).toBe(100);
    expect(parseImgNote('img: a.png w=-5')?.widthPct).toBe(5);
  });

  test('a width too large for a double is still a number the clamp catches', () => {
    // parseInt never returns NaN on `-?\d+`, but it DOES overflow to ±Infinity,
    // so the clamp is carrying a case that is easy to assume away. Both ends.
    const big = '9'.repeat(400);
    expect(parseImgNote(`img: a.png w=${big}`)?.widthPct).toBe(100);
    expect(parseImgNote(`img: a.png w=-${big}`)?.widthPct).toBe(5);
  });

  test('a width that is not an integer is not a width at all', () => {
    // `w=abc` and `w=71.5` never reach the clamp: they fail the `w=<int>`
    // match and leave the default standing. Both end at 100 — full column —
    // which is the same direction ImageRef.widthPct's floor argues for: a
    // picture shown too large beats one shown too small to see.
    expect(parseImgNote('img: a.png w=abc')?.widthPct).toBe(100);
    expect(parseImgNote('img: a.png w=71.5')?.widthPct).toBe(100);
    expect(parseImgNote('img: a.png w=')?.widthPct).toBe(100);
  });

  test('every filename the parser mints survives its own allowlist', () => {
    // The one assertion that ties the two directions together: if the naming
    // scheme in images.ts ever grows a character this class does not have,
    // every picture in the book disappears at render with nothing to show for
    // it. `p001-01.png` through a four-digit page.
    for (const f of ['p001-01.png', 'p999-99.png', 'p1000-01.png']) {
      expect(parseImgNote(`img: ${f} w=71`)?.file, f).toBe(f);
    }
  });
});

describe('isSafeImageFile', () => {
  // The allowlist as the picture folder's reader meets it: names off a
  // directory listing, which arrive as bytes and can hold what a note's
  // `\S+` never could (a newline, a NUL, a trailing space). The whole name
  // must match, from its first character to its last, on ONE line.
  test('refuses a name with anything around it, a line break included', () => {
    for (const name of ['x.png\n', '\nx.png', 'x.png\0', 'x.png ', 'a.png\n../../b']) {
      expect(`${JSON.stringify(name)}: ${isSafeImageFile(name)}`).toBe(`${JSON.stringify(name)}: false`);
    }
  });

  test('accepts the names this engine writes', () => {
    expect(isSafeImageFile('p001-01.png')).toBe(true);
  });
});

describe('rendersImage', () => {
  // The one place that decides whether a picture note becomes a figure. Three
  // gates, each able to say no on its own, and each tested with the other two
  // open so a gate that stopped working cannot hide behind its neighbours.
  const have = new Set(['p001-01.png', 'p002-01.png']);

  test('a note that passes every gate gives back the parsed picture', () => {
    expect(rendersImage('img: p001-01.png w=42', true, have)).toEqual({
      file: 'p001-01.png', widthPct: 42, fullPage: false,
    });
    expect(rendersImage('img: p002-01.png w=100 full', true, have)).toEqual({
      file: 'p002-01.png', widthPct: 100, fullPage: true,
    });
  });

  test('the option off renders nothing, even for a picture that is there', () => {
    expect(rendersImage('img: p001-01.png w=42', false, have)).toBeUndefined();
  });

  test('a hostile name renders nothing, even if the set somehow holds it', () => {
    // The allowlist is the security boundary and `have` is not: a caller
    // that put a traversal string into its set must still get no figure.
    const poisoned = new Set([...have, '../../x.png', 'a/b.png']);
    expect(rendersImage('img: ../../x.png w=42', true, poisoned)).toBeUndefined();
    expect(rendersImage('img: a/b.png w=42', true, poisoned)).toBeUndefined();
  });

  test('a name the caller cannot ship renders nothing', () => {
    // A moved .fountain, a pruned folder: the note outlives its picture. A
    // figure pointing at a file the book does not carry is an invalid EPUB.
    expect(rendersImage('img: p009-01.png w=42', true, have)).toBeUndefined();
    expect(rendersImage('img: p001-01.png w=42', true, new Set())).toBeUndefined();
  });

  test('a note that is not a picture note renders nothing', () => {
    expect(rendersImage('fmt: sans +1', true, have)).toBeUndefined();
    expect(rendersImage('a writer\'s note', true, have)).toBeUndefined();
  });
});

describe('imageNoteFiles', () => {
  // The pictures a whole text asks for, which is the question a rebuild asks
  // before it replaces a book: does the folder still hold all of them? Read
  // the way the renderers read the text, so a note they would never turn into
  // a figure is not counted as a picture the folder lost.
  test('names the picture of every picture note, once each', () => {
    const text = [
      'INT. ROOM - DAY',
      '[[img: p001-01.png w=40]]',
      'A line of action.',
      '[[img: p002-01.png w=100 full]]',
      '[[img: p001-01.png w=40]]',
    ].join('\n\n');
    expect([...imageNoteFiles(text)].sort()).toEqual(['p001-01.png', 'p002-01.png']);
  });

  test('a note no renderer would show a picture for asks for nothing', () => {
    const text = [
      'INT. ROOM - DAY',
      '[[fmt: sans]]',
      "[[a writer's note]]",
      // Refused by the allowlist inside parseImgNote.
      '[[img: ../x.png w=40]]',
      '[[img: p003-01.jpg w=40]]',
      // Folded into a line of action, where the renderers strip it as text.
      'A line with [[img: p004-01.png w=40]] folded into it.',
      // Commented out.
      '/* [[img: p005-01.png w=40]] */',
    ].join('\n\n');
    expect([...imageNoteFiles(text)]).toEqual([]);
  });
});
