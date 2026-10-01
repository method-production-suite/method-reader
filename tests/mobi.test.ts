import { afterAll, beforeAll, describe, test, expect, setSystemTime } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, posix } from 'node:path';
import { Fountain } from 'fountain-js';
import JSZip from 'jszip';
import { buildMobi } from '../src/mobi/writer';
import { mobiArt, tokensToMobiHtml } from '../src/mobi/html';
import { tokensToBody } from '../src/epub/html';
import { convertPdf } from '../src/convert';
import { calibreTool, runCalibre } from '../src/export/calibre';
import { resolveFormatOptions } from '../src/options';

const PUBLIC = new URL('./fixtures/', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-mobi-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const SAMPLE = `INT. KITCHEN - DAY

Jack enters, exhausted.

@JACK
(tired)
Long day. 5 < 6 & "quotes" too.

EXT. YARD - NIGHT

> FADE OUT.
`;

// tokensToMobiHtml takes FormatOptions with no default on purpose (same
// reason Export.swift's kfxReady has none): a call site that forgets it
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
/** Every record's bytes, record 0 first, cut at the offsets the PDB lists. */
function records(b: Uint8Array): Uint8Array[] {
  const offsets = recordOffsets(b);
  return offsets.map((o, i) => b.slice(o, offsets[i + 1] ?? b.length));
}
/** The MOBI header fields this file reads, by their offset in record 0. */
function header(b: Uint8Array) {
  const r0 = recordOffsets(b)[0];
  return {
    textRecords: u16(b, r0 + 8),
    firstNonBook: u32(b, r0 + 80),
    firstImage: u32(b, r0 + 108),
    firstContent: u16(b, r0 + 192),
    lastContent: u16(b, r0 + 194),
    fcis: u32(b, r0 + 200),
    flis: u32(b, r0 + 208),
  };
}

// The PDB header's creation and modification dates, the only bytes of a
// build that depend on when it ran (pinned below).
const PDB_DATES: [number, number] = [36, 44];
const masked = (b: Uint8Array) => {
  const c = b.slice();
  c.fill(0, ...PDB_DATES);
  return c;
};

// Enough of a PNG to tell one picture from another. The writer never looks
// inside a picture, so nothing here needs to decode.
const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const png = (...tail: number[]) => new Uint8Array([...SIGNATURE, ...tail]);

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

  test('an image note renders nothing when no pictures are handed over', () => {
    // The same three shapes as the EPUB test: standalone, deferred past a
    // speech, and glued to a line by a hand edit. Called without mobiArt's
    // answer, so there is no record for any figure to name.
    const sources = [
      'INT. A - DAY\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n',
      'INT. A - DAY\n\n@WREN\nHer line.\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n',
      'INT. A - DAY\n\n[[img: p001-01.png w=50]]\nSomeone waits.\n',
    ];
    for (const src of sources) {
      const out = html(src);
      expect(out, src).toContain('Someone waits.');
      expect(out, src).not.toContain('img:');
      expect(out, src).not.toContain('p001-01.png');
    }
  });
});

// ── pictures (registry #19a's MOBI arm) ──────────────────

describe('mobiArt', () => {
  const ON = resolveFormatOptions({});
  const OFF = resolveFormatOptions({ includeImages: false });
  const HAVE = new Set(['p001-01.png', 'p002-01.png']);
  const tokensOf = (src: string) => new Fountain().parse(src, true).tokens;

  test('lists the pictures in the order the body first shows them, each once', () => {
    // A picture shown twice is one record with two references, which is the
    // point of addressing a picture by record number.
    const src = 'INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nMid.\n\n'
      + '[[img: p001-01.png w=60]]\n\n[[img: p002-01.png w=42]]\n\nEnd.\n';
    expect(mobiArt(tokensOf(src), ON, HAVE).order).toEqual(['p002-01.png', 'p001-01.png']);
  });

  test('asks the EPUB’s question, so both books of one script show the same pictures', () => {
    // Every way a note can fail rendersImage, beside two that pass: a
    // picture nobody holds, a name the allowlist refuses even though the
    // caller claims to hold it, and a note glued to a line of action, which
    // is text and not a note token.
    const have = new Set([...HAVE, 'a/b.png']);
    const src = 'INT. LAB - DAY\n\n[[img: p009-01.png w=50]]\n\n[[img: a/b.png w=50]]\n\n'
      + '[[img: p001-01.png w=50]]\nGlued.\n\n[[img: p002-01.png w=42]]\n\n'
      + 'Mid.\n\n[[img: p001-01.png w=100 full]]\n\nEnd.\n';
    for (const format of [ON, OFF]) {
      const epub = tokensToBody(tokensOf(src), { format, images: have }).usedImages;
      expect(mobiArt(tokensOf(src), format, have).order).toEqual([...epub]);
    }
    expect(mobiArt(tokensOf(src), ON, have).order).toEqual(['p002-01.png', 'p001-01.png']);
    expect(mobiArt(tokensOf(src), OFF, have).order).toEqual([]);
  });
});

describe('MOBI figures', () => {
  const ON = resolveFormatOptions({});
  const OFF = resolveFormatOptions({ includeImages: false });
  const BREAKS = resolveFormatOptions({ scenePageBreaks: true });
  const HAVE = new Set(['p001-01.png', 'p002-01.png']);
  const render = (src: string, format = ON, have: ReadonlySet<string> = HAVE, title = true) => {
    const tokens = new Fountain().parse(src, true).tokens;
    const f = title ? format : { ...format, includeTitlePage: false };
    return tokensToMobiHtml(tokens, { title: 'T' }, f, mobiArt(tokens, f, have));
  };
  const scene = (note: string) => `INT. LAB - DAY\n\nBefore.\n\n${note}\n\nAfter.\n`;
  const breaks = (html: string) => (html.match(/<mbp:pagebreak\/>/g) ?? []).length;

  test('a picture is a centred paragraph that names its record by number, five digits from 00001', () => {
    expect(render(scene('[[img: p002-01.png w=42]]'))).toContain(
      '<p>Before.</p>\n<p align="center"><img recindex="00001" alt="Illustration"/></p>\n<p>After.</p>',
    );
  });

  test('the number is mobiArt’s: a second picture is 00002, and a repeat keeps its first number', () => {
    const html = render('INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nMid.\n\n'
      + '[[img: p001-01.png w=60]]\n\n[[img: p002-01.png w=42]]\n\nEnd.\n');
    expect([...html.matchAll(/recindex="(\d+)"/g)].map((m) => m[1])).toEqual(['00001', '00002', '00001']);
  });

  test('no width and no height: a share of the column has no spelling in this dialect', () => {
    const html = render(scene('[[img: p002-01.png w=42]]'));
    const imgs = html.match(/<img [^>]*>/g) ?? [];
    expect(imgs).toHaveLength(1);
    for (const img of imgs) expect(img).not.toMatch(/width|height|style/);
  });

  test('a picture the caller cannot ship, or pictures off, renders the picture-free body', () => {
    const plain = render('INT. LAB - DAY\n\nBefore.\n\nAfter.\n');
    expect(render(scene('[[img: p009-01.png w=42]]'))).toBe(plain);
    expect(render(scene('[[img: p002-01.png w=42]]'), ON, new Set())).toBe(plain);
    expect(render(scene('[[img: p002-01.png w=42]]'), OFF)).toBe(render('INT. LAB - DAY\n\nBefore.\n\nAfter.\n', OFF));
  });

  test('a speech before a picture is closed before it', () => {
    expect(render('INT. LAB - DAY\n\n@WREN\nHer line.\n\n[[img: p002-01.png w=42]]\n\nAfter.\n')).toContain(
      '<blockquote><b>WREN</b><br/>Her line.</blockquote>\n<p align="center"><img recindex="00001"',
    );
  });

  test('a figure declines a pending page marker, which lands on the next text instead', () => {
    const html = render('INT. LAB - DAY\n\nBefore.\n\n= pg 7\n\n[[img: p002-01.png w=42]]\n\nAfter.\n');
    expect(html).toContain('<p align="center"><img recindex="00001" alt="Illustration"/></p>');
    expect(html).toContain('<p><span class="page-marker"><font size="-2">7.</font></span>After.</p>');
  });

  describe('a full-page plate', () => {
    // The EPUB's p.figure-full, in this dialect's one break primitive: a page
    // of its own, and nothing shares it.
    test('ends the page before it and the page it is on', () => {
      const html = render(scene('[[img: p001-01.png w=100 full]]'));
      expect(html).toContain('<p>Before.</p>\n<mbp:pagebreak/>\n<p align="center"><img recindex="00001" alt="Illustration"/></p>\n<mbp:pagebreak/>\n<p>After.</p>');
    });

    test('an ordinary picture breaks nothing', () => {
      expect(breaks(render(scene('[[img: p002-01.png w=42]]')))).toBe(breaks(render(scene(''))));
    });

    test('at the start of the book it has nothing to break from, so there is no blank page in front', () => {
      const card = '[[img: p001-01.png w=100 full]]\n\nINT. LAB - DAY\n\nWork.\n';
      // After the title page's own break: one break, then the plate.
      const titled = render(card);
      expect(titled).toContain('</center>\n<mbp:pagebreak/>\n<p align="center"><img recindex="00001"');
      expect(breaks(titled)).toBe(2);
      // With no title page the plate opens the file.
      expect(render(card, ON, HAVE, false)).toContain('<body>\n<p align="center"><img recindex="00001"');
    });

    test('at the end of the book it has nothing to break away from, so there is no blank page after', () => {
      const html = render('INT. LAB - DAY\n\nWork.\n\n[[img: p001-01.png w=100 full]]\n');
      expect(html).toMatch(/<img recindex="00001" alt="Illustration"\/><\/p>\n<\/body>/);
    });

    test('never two breaks in a row: between two plates, or before a scene that breaks anyway', () => {
      const src = 'Opening.\n\n[[img: p001-01.png w=100 full]]\n\n[[img: p002-01.png w=100 full]]\n\n'
        + 'INT. LAB - DAY\n\nWork.\n\n[[img: p001-01.png w=100 full]]\n\nEXT. YARD - NIGHT\n\nRest.\n';
      for (const format of [ON, BREAKS]) {
        const html = render(src, format);
        expect(html).not.toMatch(/<mbp:pagebreak\/>\s*<mbp:pagebreak\/>/);
        expect(html).toContain('<img recindex="00001" alt="Illustration"/></p>\n<mbp:pagebreak/>\n<p align="center"><img recindex="00002"');
      }
      // Title page, before the first plate, between the plates, after the
      // second, then before and after the third: six either way, since each
      // scene heading follows a plate and its own break is the plate's.
      expect(breaks(render(src, ON))).toBe(6);
      expect(breaks(render(src, BREAKS))).toBe(6);
    });
  });
});

describe('MOBI image records', () => {
  const build = (images?: Uint8Array[]) =>
    buildMobi({ title: 'Test Script', author: 'Jane Doe', html: sampleHtml(), images });
  const PICTURES = [png(1, 2, 3), png(4, 5)];

  test('each picture is a record of its own, after the text and before FLIS, bytes untouched', () => {
    const mobi = build(PICTURES);
    const recs = records(mobi);
    const { textRecords } = header(mobi);
    expect(recs).toHaveLength(1 + textRecords + PICTURES.length + 3);
    expect([...recs[1 + textRecords]]).toEqual([...PICTURES[0]]);
    expect([...recs[2 + textRecords]]).toEqual([...PICTURES[1]]);
    expect(ascii(recs[3 + textRecords], 0, 4)).toBe('FLIS');
  });

  test('the PalmDOC record count still counts text alone, and the text still reassembles', () => {
    const mobi = build(PICTURES);
    const { textRecords } = header(mobi);
    expect(textRecords).toBe(header(build()).textRecords);
    const text = records(mobi).slice(1, 1 + textRecords);
    const joined = new Uint8Array(text.reduce((n, r) => n + r.length, 0));
    text.reduce((at, r) => (joined.set(r, at), at + r.length), 0);
    expect(new TextDecoder().decode(joined)).toBe(sampleHtml());
  });

  test('108, the first image record, is an absolute record number: the one after the text', () => {
    const mobi = build(PICTURES);
    const h = header(mobi);
    expect(h.firstImage).toBe(1 + h.textRecords);
    expect([...records(mobi)[h.firstImage]]).toEqual([...PICTURES[0]]);
  });

  test('194, the last content record, reaches past the text to the last picture', () => {
    const mobi = build(PICTURES);
    const h = header(mobi);
    expect(h.firstContent).toBe(1);
    expect(h.lastContent).toBe(h.textRecords + PICTURES.length);
    expect([...records(mobi)[h.lastContent]]).toEqual([...PICTURES[1]]);
  });

  test('80, the first non-book record, is the first record that is not text: now a picture', () => {
    const mobi = build(PICTURES);
    const h = header(mobi);
    expect(h.firstNonBook).toBe(1 + h.textRecords);
    expect([...records(mobi)[h.firstNonBook]]).toEqual([...PICTURES[0]]);
  });

  test('FLIS and FCIS move down past the pictures, and the header still names them', () => {
    const mobi = build(PICTURES);
    const h = header(mobi);
    const recs = records(mobi);
    expect(h.flis).toBe(1 + h.textRecords + PICTURES.length);
    expect(ascii(recs[h.flis], 0, 4)).toBe('FLIS');
    expect(h.fcis).toBe(h.flis + 1);
    expect(ascii(recs[h.fcis], 0, 4)).toBe('FCIS');
  });

  test('a large picture rides whole', () => {
    // Bigger than a text record by three orders of magnitude, and past
    // anything a 16-bit length could hold.
    const big = new Uint8Array(2_000_000).map((_, i) => (i * 31) & 0xff);
    const mobi = build([big]);
    const h = header(mobi);
    expect(records(mobi)[h.firstImage]).toEqual(big);
  });

  test('without pictures, 108 says none and 80 and 194 are where the text ends', () => {
    const mobi = build();
    const h = header(mobi);
    expect(h.firstImage).toBe(0xffffffff);
    expect(h.lastContent).toBe(h.textRecords);
    expect(h.firstNonBook).toBe(1 + h.textRecords);
    expect(h.flis).toBe(h.firstNonBook);
  });

  test('the clock moves the PDB dates and nothing else', () => {
    // Which is what lets the identity pin below mask those eight bytes and
    // no others.
    let early: Uint8Array;
    let late: Uint8Array;
    try {
      setSystemTime(new Date('2020-01-01T00:00:00Z'));
      early = build(PICTURES);
      setSystemTime(new Date('2026-10-01T12:34:56Z'));
      late = build(PICTURES);
    } finally {
      setSystemTime();
    }
    expect(late.length).toBe(early.length);
    const moved = [...early.keys()].filter((i) => early[i] !== late[i]);
    expect(moved.length).toBeGreaterThan(0);
    expect(moved.every((i) => i >= PDB_DATES[0] && i < PDB_DATES[1])).toBe(true);
  });

  test('a book with no pictures is byte for byte the book from before image records, dates aside', () => {
    // SHA-256 of this book with the PDB dates zeroed, captured from the
    // writer and renderer at b63ae74, the last commit before either could
    // carry a picture. No pictures and an empty list are the same book.
    const BEFORE = 'c77dd2c56516cc252110b3d5bf3cea230f66c42e869c0534fbe8f89e3826d53c';
    const sha = (b: Uint8Array) => createHash('sha256').update(masked(b)).digest('hex');
    expect(sha(build())).toBe(BEFORE);
    expect(sha(build([]))).toBe(BEFORE);
  });
});

describe('images.pdf in the MOBI', () => {
  // images.pdf's own .fountain and the pictures its conversion carried,
  // rendered and packed here directly, as tests/epub-images.test.ts does
  // for the EPUB.
  const ON = resolveFormatOptions({});
  let fountainText = '';
  let meta: { title: string; author?: string } = { title: '' };
  let pictures: ReadonlyMap<string, Uint8Array> = new Map();
  beforeAll(async () => {
    const result = await convertPdf(new Uint8Array(await Bun.file(`${PUBLIC}images.pdf`).arrayBuffer()));
    fountainText = result.fountainText;
    meta = { title: result.meta.title, author: result.meta.author };
    pictures = new Map(result.images.map((img) => [img.name, img.bytes]));
  }, 60000);

  const book = () => {
    const tokens = new Fountain().parse(fountainText, true).tokens;
    const art = mobiArt(tokens, ON, new Set(pictures.keys()));
    const html = tokensToMobiHtml(tokens, meta, ON, art);
    const images = art.order.map((file) => pictures.get(file)!);
    return { html, mobi: buildMobi({ ...meta, html, images }) };
  };
  // The script's own order, read off the .fountain rather than asked of
  // mobiArt, so the checks below do not grade mobiArt against itself.
  const inScriptOrder = () => [...fountainText.matchAll(/^\[\[img: (\S+)/gm)].map((m) => pictures.get(m[1])!);

  test('every recindex in the markup opens the record holding that very picture', () => {
    const { html, mobi } = book();
    const refs = [...html.matchAll(/<img recindex="(\d{5})"/g)].map((m) => Number(m[1]));
    expect(refs).toEqual([1, 2]);
    expect([...pictures.keys()]).toEqual(['p001-01.png', 'p002-01.png']);
    const recs = records(mobi);
    const { firstImage } = header(mobi);
    const want = inScriptOrder();
    expect(want).toHaveLength(2);
    refs.forEach((n, i) => expect(recs[firstImage + n - 1]).toEqual(want[i]));
  });

  // Calibre's MOBI reader, run on our own file: an outside reader of the
  // container rather than this file's helpers. Only ever inside bun test,
  // where tests/isolate-calibre-config.ts has pointed Calibre at a scratch
  // copy of its settings, and process.env is what carries that to the spawn.
  const EBOOK_CONVERT = calibreTool('ebook-convert');
  const withCalibre = EBOOK_CONVERT ? test : test.skip;
  const NO_CALIBRE = EBOOK_CONVERT ? '' : ' (skipped: Calibre’s ebook-convert was not found)';

  withCalibre(`Calibre reads every picture back, byte for byte, in order, each from the right <img>${NO_CALIBRE}`, async () => {
    const mobiPath = join(SCRATCH, 'images.mobi');
    const epubPath = join(SCRATCH, 'images-from-mobi.epub');
    writeFileSync(mobiPath, book().mobi);
    // No generated cover: it would be a picture this book never had.
    await runCalibre(EBOOK_CONVERT!, [mobiPath, epubPath, '--no-default-epub-cover'], process.env);

    const zip = await JSZip.loadAsync(readFileSync(epubPath));
    const container = await zip.file('META-INF/container.xml')!.async('string');
    const opfPath = /full-path="([^"]+)"/.exec(container)![1];
    const opf = await zip.file(opfPath)!.async('string');
    const hrefs = new Map<string, string>();
    for (const [item] of opf.matchAll(/<item\b[^>]*>/g)) {
      const id = /\bid="([^"]+)"/.exec(item)?.[1];
      const href = /\bhref="([^"]+)"/.exec(item)?.[1];
      if (id && href) hrefs.set(id, posix.join(posix.dirname(opfPath), decodeURIComponent(href)));
    }
    const shown: Uint8Array[] = [];
    for (const [, idref] of opf.matchAll(/<itemref\b[^>]*\bidref="([^"]+)"/g)) {
      const doc = hrefs.get(idref)!;
      const xhtml = await zip.file(doc)!.async('string');
      for (const [, src] of xhtml.matchAll(/<img\b[^>]*\bsrc="([^"]+)"/g)) {
        const file = posix.join(posix.dirname(doc), decodeURIComponent(src));
        shown.push(await zip.file(file)!.async('uint8array'));
      }
    }
    expect(shown).toEqual(inScriptOrder());
  }, 120000);
});
