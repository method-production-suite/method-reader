import { describe, test, expect } from 'bun:test';
import { Fountain } from 'fountain-js';
import { buildMobi } from '../src/mobi/writer';
import { tokensToMobiHtml } from '../src/mobi/html';
import { resolveFormatOptions } from '../src/options';

const SAMPLE = `INT. KITCHEN - DAY

Jack enters, exhausted.

@JACK
(tired)
Long day. 5 < 6 & "quotes" too.

EXT. YARD - NIGHT

> FADE OUT.
`;

// tokensToMobiHtml takes FormatOptions with no default on purpose (same
// reason the Swift app's kfxReady had none): a call site that forgets it
// would silently render with scenePageBreaks off rather than fail.
const OPTS = resolveFormatOptions({});

function sampleHtml(): string {
  const { tokens } = new Fountain().parse(SAMPLE, true);
  return tokensToMobiHtml(tokens, { title: 'Test Script', author: 'Jane Doe' }, OPTS);
}

// ── binary reader helpers (independent re-parse of our own output) ──

function u16(b: Uint8Array, o: number): number {
  return (b[o] << 8) | b[o + 1];
}
function u32(b: Uint8Array, o: number): number {
  return ((b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3]) >>> 0;
}
function ascii(b: Uint8Array, o: number, len: number): string {
  return new TextDecoder('ascii').decode(b.slice(o, o + len));
}
function recordOffsets(b: Uint8Array): number[] {
  const count = u16(b, 76);
  const offsets: number[] = [];
  for (let i = 0; i < count; i++) offsets.push(u32(b, 78 + i * 8));
  return offsets;
}

// ── MOBI HTML mapping ────────────────────────────────────

describe('tokensToMobiHtml', () => {
  test('title page is centered and followed by a page break', () => {
    const html = sampleHtml();
    expect(html).toContain('<center>');
    expect(html).toContain('Test Script');
    expect(html).toContain('Jane Doe');
    expect(html).toContain('<mbp:pagebreak/>');
  });

  test('scene headings are bold paragraphs', () => {
    expect(sampleHtml()).toContain('<p><b>INT. KITCHEN - DAY</b></p>');
  });

  test('dialogue renders as a blockquote with bold cue and italic parenthetical', () => {
    const html = sampleHtml();
    expect(html).toMatch(/<blockquote><b>JACK<\/b><br\/><i>\(tired\)<\/i><br\/>Long day\./);
  });

  test('transitions are right-aligned without the forcing prefix', () => {
    expect(sampleHtml()).toContain('<p align="right">FADE OUT.</p>');
  });

  test('text is HTML-escaped', () => {
    expect(sampleHtml()).toContain('5 &lt; 6 &amp; &quot;quotes&quot; too.');
  });

  test('mini-slugs read as bold paragraphs too — this dialect has no third weight', () => {
    const tokens = new Fountain().parse('INT. STORE - NIGHT\n\nThe gate rattles.\n\n.LATER\n\nStill on.\n', true).tokens;
    const html = tokensToMobiHtml(tokens, { title: 'T' }, OPTS);
    expect(html).toContain('<p><b>LATER</b></p>');
  });

  test('scenePageBreaks emits mbp:pagebreak before every scene heading except the first', () => {
    const src = 'INT. A - DAY\n\nAction.\n\nINT. B - NIGHT\n\nMore.\n';
    const { tokens } = new Fountain().parse(src, true);
    const off = tokensToMobiHtml(tokens, { title: 'T' }, OPTS);
    const on = tokensToMobiHtml(tokens, { title: 'T' }, resolveFormatOptions({ scenePageBreaks: true }));
    // The title page always contributes exactly one pagebreak of its own.
    expect(off.match(/<mbp:pagebreak\/>/g)!.length).toBe(1);
    expect(on.match(/<mbp:pagebreak\/>/g)!.length).toBe(2);
  });

  test('includeTitlePage off drops the MOBI title page and its break', () => {
    const { tokens } = new Fountain().parse(SAMPLE, true);
    const on = tokensToMobiHtml(tokens, { title: 'Test Script', author: 'Jane Doe' }, OPTS);
    expect(on).toContain('Test Script');
    expect(on).toContain('Written by');
    expect(on.match(/<mbp:pagebreak\/>/g) ?? []).toHaveLength(1);

    const off = tokensToMobiHtml(
      tokens,
      { title: 'Test Script', author: 'Jane Doe' },
      resolveFormatOptions({ includeTitlePage: false }),
    );
    expect(off).not.toContain('Written by');
    expect(off).not.toMatch(/<font size="\+2">/);
    // No title page means no title-page break either — the body must not
    // open on a blank leading page.
    expect(off.match(/<mbp:pagebreak\/>/g) ?? []).toHaveLength(0);
  });

  test('scenePageBreaks breaks before the first scene when body content precedes it', () => {
    // Opening action before the first slugline: the EPUB separates it with
    // section.scene's page-break-before, so MOBI must too. Suppressing the
    // break is only correct when nothing has been emitted since the last
    // one (title page, or the very start of a title-less book).
    const src = 'Black screen. A hum builds.\n\nINT. A - DAY\n\nAction.\n';
    const { tokens } = new Fountain().parse(src, true);
    const on = tokensToMobiHtml(
      tokens,
      { title: 'T' },
      resolveFormatOptions({ scenePageBreaks: true }),
    );
    // Title-page break, then one before INT. A because action preceded it.
    expect(on.match(/<mbp:pagebreak\/>/g) ?? []).toHaveLength(2);
    expect(on).toMatch(/<mbp:pagebreak\/>\s*<p><b>INT\. A - DAY<\/b><\/p>/);
  });

  test('scenePageBreaks leaves no blank leading page when a scene opens the book', () => {
    const src = 'INT. A - DAY\n\nAction.\n';
    const { tokens } = new Fountain().parse(src, true);
    const withTitle = tokensToMobiHtml(
      tokens,
      { title: 'T' },
      resolveFormatOptions({ scenePageBreaks: true }),
    );
    expect(withTitle.match(/<mbp:pagebreak\/>/g) ?? []).toHaveLength(1);

    const noTitle = tokensToMobiHtml(
      tokens,
      { title: 'T' },
      resolveFormatOptions({ scenePageBreaks: true, includeTitlePage: false }),
    );
    expect(noTitle.match(/<mbp:pagebreak\/>/g) ?? []).toHaveLength(0);
  });

  test('scenePageBreaks does not break before a mini-slug, only before primary scenes (registry #5b)', () => {
    // fountain-js strips the leading dot: `.LATER` tokenizes as a
    // scene_heading with text "LATER", same as the EPUB side relies on.
    const src = 'INT. A - DAY\n\nAction.\n\n.LATER\n\nMore.\n\nINT. B - NIGHT\n\nEnd.\n';
    const { tokens } = new Fountain().parse(src, true);
    expect(tokens.find((t) => t.text === 'LATER')?.type).toBe('scene_heading');
    const on = tokensToMobiHtml(tokens, { title: 'T' }, resolveFormatOptions({ scenePageBreaks: true }));
    // Title page's break, plus one before INT. B — none before LATER.
    expect(on.match(/<mbp:pagebreak\/>/g)!.length).toBe(2);
    expect(on).not.toMatch(/<mbp:pagebreak\/>\s*<p><b>LATER<\/b><\/p>/);
  });
});

// ── MOBI container ───────────────────────────────────────

describe('buildMobi', () => {
  const build = () => buildMobi({ title: 'Test Script', author: 'Jane Doe', html: sampleHtml() });

  test('PDB header carries BOOK/MOBI type and consistent record offsets', () => {
    const mobi = build();
    expect(ascii(mobi, 60, 8)).toBe('BOOKMOBI');
    const offsets = recordOffsets(mobi);
    expect(offsets.length).toBeGreaterThanOrEqual(5); // rec0 + text + FLIS + FCIS + EOF
    for (let i = 1; i < offsets.length; i++) {
      expect(offsets[i]).toBeGreaterThan(offsets[i - 1]);
    }
    expect(offsets[offsets.length - 1]).toBeLessThan(mobi.length);
  });

  test('record 0 has uncompressed PalmDOC and a MOBI 6 header', () => {
    const mobi = build();
    const r0 = recordOffsets(mobi)[0];
    expect(u16(mobi, r0)).toBe(1); // compression: none
    expect(ascii(mobi, r0 + 16, 4)).toBe('MOBI');
    expect(u32(mobi, r0 + 24)).toBe(2); // mobi type: book
    expect(u32(mobi, r0 + 28)).toBe(65001); // UTF-8
    expect(u32(mobi, r0 + 36)).toBe(6); // version
  });

  test('text records reassemble to the exact source HTML', () => {
    const mobi = build();
    const html = sampleHtml();
    const offsets = recordOffsets(mobi);
    const r0 = offsets[0];
    const textLength = u32(mobi, r0 + 4);
    const textRecords = u16(mobi, r0 + 8);
    expect(textLength).toBe(new TextEncoder().encode(html).length);

    let bytes = new Uint8Array(0);
    for (let i = 1; i <= textRecords; i++) {
      const end = offsets[i + 1] ?? mobi.length;
      const merged = new Uint8Array(bytes.length + end - offsets[i]);
      merged.set(bytes);
      merged.set(mobi.slice(offsets[i], end), bytes.length);
      bytes = merged;
    }
    expect(new TextDecoder().decode(bytes.slice(0, textLength))).toBe(html);
  });

  test('full name and EXTH metadata are present', () => {
    const mobi = build();
    const r0 = recordOffsets(mobi)[0];
    const nameOffset = u32(mobi, r0 + 84);
    const nameLength = u32(mobi, r0 + 88);
    expect(ascii(mobi, r0 + nameOffset, nameLength)).toBe('Test Script');
    expect(ascii(mobi, r0 + 248, 4)).toBe('EXTH');
    const whole = new TextDecoder('latin1').decode(mobi);
    expect(whole).toContain('Jane Doe'); // EXTH 100
    expect(whole).toContain('EBOK'); // EXTH 501
  });

  test('FLIS, FCIS, and EOF trailer records are in place', () => {
    const mobi = build();
    const offsets = recordOffsets(mobi);
    const n = offsets.length;
    expect(ascii(mobi, offsets[n - 3], 4)).toBe('FLIS');
    expect(ascii(mobi, offsets[n - 2], 4)).toBe('FCIS');
    expect([...mobi.slice(offsets[n - 1], offsets[n - 1] + 4)]).toEqual([0xe9, 0x8e, 0x0d, 0x0a]);
  });

  test('record unique ids stay unique past 128 text records', () => {
    // >512KB of HTML forces >128 4KB text records — the length of a
    // feature shooting script with dual dialogue. A unique id written as
    // a single byte wraps at record 128 and collides from there on.
    const long = Array.from(
      { length: 20000 },
      (_, i) => `<p>Scene line ${i}, padding the record count well past the byte boundary.</p>`
    ).join('\n');
    const mobi = buildMobi({ title: 'Very Long', author: 'A', html: long });
    const count = u16(mobi, 76);
    expect(count).toBeGreaterThan(128);
    const uids: number[] = [];
    for (let i = 0; i < count; i++) {
      const o = 78 + i * 8 + 5; // entry: offset u32, attributes u8, uid u24
      uids.push((mobi[o] << 16) | (mobi[o + 1] << 8) | mobi[o + 2]);
    }
    expect(new Set(uids).size).toBe(count);
  }, 20000);

  test('multi-record text never splits a UTF-8 codepoint', () => {
    // >4096 bytes of curly-quoted text forces multiple records.
    const long = Array.from({ length: 300 }, (_, i) => `<p>Line ${i} — “quoted” he said.</p>`).join('\n');
    const mobi = buildMobi({ title: 'Long', author: 'A', html: long });
    const offsets = recordOffsets(mobi);
    const r0 = offsets[0];
    const textRecords = u16(mobi, r0 + 8);
    expect(textRecords).toBeGreaterThan(1);
    const decoder = new TextDecoder('utf-8', { fatal: true });
    for (let i = 1; i <= textRecords; i++) {
      const end = offsets[i + 1] ?? mobi.length;
      // fatal decoder throws if a record starts/ends mid-codepoint
      expect(() => decoder.decode(mobi.slice(offsets[i], end))).not.toThrow();
    }
  });
});

describe('MOBI page markers', () => {
  // Markers ride inside the NEXT block rather than taking a line of their
  // own — same rule as the EPUB path. MOBI 6 has no stylesheet, so the
  // number is sized with a <font> wrapper (this file's convention) and
  // carries none of EPUB3's pagebreak semantics.
  function markerHtml(source: string): string {
    const { tokens } = new Fountain().parse(source, true);
    return tokensToMobiHtml(tokens, { title: 'T', author: 'A' }, OPTS);
  }

  test('marker rides inside the following block, not its own paragraph', () => {
    const html = markerHtml(`INT. KITCHEN - DAY

= pg 47

Jack enters, exhausted.
`);
    expect(html).not.toMatch(/<p class="page-marker">/);
    expect(html).toMatch(/page-marker/);
    // it sits inside a block, not standing alone between two closing tags
    expect(html).toMatch(/<(p|h[1-6])[^>]*>\s*<span class="page-marker">/);
  });

  test('carries no EPUB3 pagebreak semantics', () => {
    const html = markerHtml(`INT. KITCHEN - DAY

= pg 47

Jack enters.
`);
    expect(html).not.toMatch(/epub:type/);
    expect(html).not.toMatch(/doc-pagebreak/);
    expect(html).not.toMatch(/id="pg47"/);
  });

  test('a trailing marker with nothing after it is dropped', () => {
    const html = markerHtml(`INT. KITCHEN - DAY

Jack enters.

= pg 48
`);
    expect(html).not.toMatch(/page-marker/);
  });
});

describe('inline emphasis', () => {
  const html = (src: string) => {
    const { tokens } = new Fountain().parse(src, true);
    return tokensToMobiHtml(tokens, { title: 'T' }, resolveFormatOptions({}));
  };

  test('underscores become <u>', () => {
    // Untested since it landed in the v0.4.2 merge train; rich-formatting
    // phase 1 is the first thing that emits these underscores from a PDF
    // rather than from a hand edit, so it stops being decorative here.
    expect(html('INT. A - DAY\n\nThe sign reads _DO NOT ENTER_.\n')).toContain(
      '<u>DO NOT ENTER</u>',
    );
  });

  test('mixed marks unwrap with the underscore innermost', () => {
    // The nesting joinLine emits for a bold+underlined run. Stars are
    // replaced before underscores, so this composes without a special case.
    expect(html('INT. A - DAY\n\nThe stamp is **_VOID_** now.\n')).toContain(
      '<b><u>VOID</u></b>',
    );
    expect(html('INT. A - DAY\n\nAll three: ***_LOUD_*** here.\n')).toContain(
      '<b><i><u>LOUD</u></i></b>',
    );
  });

  test('bold, italic and bold-italic keep their existing tags', () => {
    const out = html('INT. A - DAY\n\nA ***b1*** and **c1** and *d1*.\n');
    expect(out).toContain('<b><i>b1</i></b>');
    expect(out).toContain('<b>c1</b>');
    expect(out).toContain('<i>d1</i>');
  });
});

describe('font shifts', () => {
  const html = (src: string, format = resolveFormatOptions({})) => {
    const { tokens } = new Fountain().parse(src, true);
    return tokensToMobiHtml(tokens, { title: 'T' }, format);
  };

  test('a size step wraps the action in a font tag', () => {
    expect(html('INT. A - DAY\n\n[[fmt: +2]]\nA SIGN.\n')).toContain(
      '<p><font size="+2">A SIGN.</font></p>',
    );
  });

  test('all three steps map', () => {
    expect(html('INT. A - DAY\n\n[[fmt: -1]]\nSmall.\n')).toContain('<font size="-1">');
    expect(html('INT. A - DAY\n\n[[fmt: +1]]\nMedium.\n')).toContain('<font size="+1">');
  });

  test('dialogue size steps land too', () => {
    expect(html('INT. A - DAY\n\n@WREN\n[[fmt: +1]] Read it back.\n')).toContain(
      '<font size="+1">Read it back.</font>',
    );
  });

  test('family is dropped: MOBI 6 face support is not reliable', () => {
    const out = html('INT. A - DAY\n\n[[fmt: sans]]\nCHYRON.\n');
    expect(out).toContain('<p>CHYRON.</p>');
    expect(out).not.toContain('face=');
    expect(out).not.toContain('fmt-sans');
  });

  test('a family+size note keeps the size and drops the family', () => {
    expect(html('INT. A - DAY\n\n[[fmt: sans +2]]\nA SIGN.\n')).toContain(
      '<p><font size="+2">A SIGN.</font></p>',
    );
  });

  test('with the knob off the note vanishes and no font tag appears', () => {
    const out = html(
      'INT. A - DAY\n\n[[fmt: +2]]\nA SIGN.\n',
      resolveFormatOptions({ preserveFontShifts: false }),
    );
    expect(out).toContain('<p>A SIGN.</p>');
    // Scoped to the body: the generated title page sets its own title in
    // <font size="+2">, so a document-wide check would never fail.
    expect(out).not.toContain('<font size="+2">A SIGN.');
  });

  test('notes are invisible here too', () => {
    const out = html('INT. A - DAY\n\nHe leaves [[check this]] quickly.\n');
    expect(out).toContain('He leaves quickly.');
    expect(out).not.toContain('[[');
  });
});
