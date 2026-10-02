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
import { exth, exthRecords } from './helpers/mobi-records';

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

/**
 * A real JPEG, small enough to spell out: mid grey (every sample 128), eight
 * pixels tall and eight per block wide, so two block counts are two different
 * pictures. Baseline, one grey component, every quantiser 1, and Huffman
 * tables with one code each: a block of 128 has a DC difference of 0 and no
 * AC coefficients, so each block is the two bits "0" (category 0) and "0"
 * (end of block), padded with ones. The JFIF segment sits where calibre's
 * format sniffer looks for it. Calibre decodes them in the round trip below,
 * and so did macOS's ImageIO when they were made (2026-10-01); the writer
 * never looks inside a picture.
 */
function greyJpeg(blocks: number): Uint8Array {
  const segment = (marker: number, body: number[]) => [0xff, marker, (body.length + 2) >> 8, (body.length + 2) & 0xff, ...body];
  const width = 8 * blocks;
  const bits = '00'.repeat(blocks).padEnd(Math.ceil((2 * blocks) / 8) * 8, '1');
  const scan = (bits.match(/.{8}/g) ?? []).map((b) => parseInt(b, 2));
  const oneCodeOfLengthOne = [1, ...Array(15).fill(0), 0];
  return new Uint8Array([
    0xff, 0xd8, // SOI
    ...segment(0xe0, [0x4a, 0x46, 0x49, 0x46, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0]), // APP0 JFIF 1.1
    ...segment(0xdb, [0, ...Array(64).fill(1)]), // DQT 0
    ...segment(0xc0, [8, 0, 8, width >> 8, width & 0xff, 1, 1, 0x11, 0]), // SOF0
    ...segment(0xc4, [0x00, ...oneCodeOfLengthOne]), // DHT, DC 0
    ...segment(0xc4, [0x10, ...oneCodeOfLengthOne]), // DHT, AC 0
    ...segment(0xda, [1, 1, 0, 0, 63, 0]), // SOS
    ...scan,
    0xff, 0xd9, // EOI
  ]);
}

// Calibre's MOBI reader, run on our own files: an outside reader of the
// container rather than this file's helpers. Only ever inside bun test,
// where tests/isolate-calibre-config.ts has pointed Calibre at a scratch copy
// of its settings, and process.env is what carries that to the spawn.
const EBOOK_CONVERT = calibreTool('ebook-convert');
const withCalibre = EBOOK_CONVERT ? test : test.skip;
const NO_CALIBRE = EBOOK_CONVERT ? '' : ' (skipped: Calibre’s ebook-convert was not found)';

/** A MOBI converted to EPUB by Calibre, opened for reading back. */
async function throughCalibre(mobi: Uint8Array, name: string) {
  const mobiPath = join(SCRATCH, `${name}.mobi`);
  const epubPath = join(SCRATCH, `${name}-from-mobi.epub`);
  writeFileSync(mobiPath, mobi);
  // No generated cover: it would be a picture this book never had, and it
  // would hide whether the book's own cover came through.
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
  const bytesOf = (path: string) => zip.file(path)!.async('uint8array');
  return {
    opf,
    /** Every picture an `<img>` on a spine page shows, in reading order. */
    async shown(): Promise<Uint8Array[]> {
      const out: Uint8Array[] = [];
      for (const [, idref] of opf.matchAll(/<itemref\b[^>]*\bidref="([^"]+)"/g)) {
        const doc = hrefs.get(idref)!;
        const xhtml = await zip.file(doc)!.async('string');
        for (const [, src] of xhtml.matchAll(/<img\b[^>]*\bsrc="([^"]+)"/g)) {
          out.push(await bytesOf(posix.join(posix.dirname(doc), decodeURIComponent(src))));
        }
      }
      return out;
    },
    coverMeta() {
      const id = /<meta\b[^>]*\bname="cover"[^>]*\bcontent="([^"]+)"/.exec(opf)?.[1]
        ?? /<meta\b[^>]*\bcontent="([^"]+)"[^>]*\bname="cover"/.exec(opf)?.[1];
      return { id, href: id ? hrefs.get(id) : undefined };
    },
    /** The picture the package names as its cover, or undefined. */
    async cover(): Promise<Uint8Array | undefined> {
      const { href } = this.coverMeta();
      return href && zip.file(href) ? bytesOf(href) : undefined;
    },
  };
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

  describe('the cover', () => {
    const CARD = '[[img: p001-01.png w=100 full]]\n\nINT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nEnd.\n';

    test('is the title card pickCover chose, named by its place in the order, which still holds it', () => {
      // The card keeps its record because the MOBI keeps it on its page; the
      // cover is a second use of that record, not a picture of its own.
      const art = mobiArt(tokensOf(CARD), ON, HAVE);
      expect(art.order).toEqual(['p001-01.png', 'p002-01.png']);
      expect(art.coverIndex).toBe(0);
    });

    test('asks pickCover, so the MOBI and the EPUB of one script wear the same cover or none', () => {
      // Every way a script can fail to have a cover, beside the one that has
      // one: text, a heading or a smaller picture ahead of the card, a card
      // the caller holds no bytes for, and each option off.
      const sources = [
        CARD,
        `Opening line.\n\n${CARD}`,
        `INT. HALL - DAY\n\n${CARD}`,
        `[[img: p002-01.png w=42]]\n\n${CARD}`,
        '[[img: p009-01.png w=100 full]]\n\nINT. LAB - DAY\n\nEnd.\n',
      ];
      const formats = [ON, OFF, resolveFormatOptions({ promoteCoverImage: false })];
      let covers = 0;
      for (const src of sources) {
        for (const format of formats) {
          const art = mobiArt(tokensOf(src), format, HAVE);
          const epub = tokensToBody(tokensOf(src), { format, images: HAVE }).coverImage;
          const mobi = art.coverIndex === undefined ? undefined : art.order[art.coverIndex];
          expect(mobi, `${format.promoteCoverImage}/${format.includeImages}: ${src}`).toBe(epub);
          if (mobi) covers++;
        }
      }
      // The control: exactly one script with both options on has a cover,
      // without which every case agreeing on "none" would pass.
      expect(covers).toBe(1);
    });
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

    test('a title card that is the cover stays on its page, where the EPUB leaves it out', () => {
      // The EPUB's pages skip the card that became its cover. This book draws
      // it: calibre's MOBI reader finds the cover only when it is a JPEG
      // (registry #19b), so a card left out of the flow could vanish from the
      // book entirely. The worst case kept is the card met twice.
      const card = '[[img: p001-01.png w=100 full]]\n\nINT. LAB - DAY\n\nWork.\n';
      const tokens = new Fountain().parse(card, true).tokens;
      const cover = mobiArt(tokens, ON, HAVE);
      expect(cover.coverIndex).toBe(0);
      const html = render(card);
      expect(html).toContain('</center>\n<mbp:pagebreak/>\n<p align="center"><img recindex="00001" alt="Illustration"/></p>\n<mbp:pagebreak/>\n<p><b>INT. LAB - DAY</b></p>');
      // The cover changes nothing on the pages: the markup is the markup with
      // the cover switch off.
      expect(html).toBe(render(card, resolveFormatOptions({ promoteCoverImage: false })));
      // And the EPUB of the same script does leave it out, which is the
      // difference this test pins.
      expect(tokensToBody(tokens, { format: ON, images: HAVE }).usedImages.size).toBe(0);
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

  test('a recindex the records cannot answer is a caller bug, and says which', () => {
    // A reader finds recindex N at the first image record plus N - 1 and
    // takes whatever record sits there. Past the last picture that is FLIS,
    // and 00000 is the last text record: a wrong record on the page, never a
    // missing picture, so nothing downstream would ever notice.
    const showing = (...ns: string[]) =>
      ns.map((n) => `<p align="center"><img recindex="${n}" alt="Illustration"/></p>`).join('\n');
    const at = (html: string, images?: Uint8Array[]) => () => buildMobi({ title: 'T', html, images });
    expect(at(showing('00001', '00003'), PICTURES)).toThrow(/recindex="00003"/);
    expect(at(showing('00000', '00002'), PICTURES)).toThrow(/recindex="00000"/);
    // With no pictures at all, every number is out of range.
    expect(at(showing('00001'))).toThrow(/recindex="00001"/);
    expect(at(showing('00001'), [])).toThrow(/recindex="00001"/);
    // The control: every number the records answer builds.
    expect(at(showing('00001', '00002', '00001'), PICTURES)).not.toThrow();
  });

  test('a large picture rides whole', () => {
    // About 490 text records' worth in one record, and past anything a
    // 16-bit length could hold.
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
    // writer and renderer at 0c71095, the last commit before either could
    // carry a picture. No pictures and an empty list are the same book.
    const BEFORE = 'c77dd2c56516cc252110b3d5bf3cea230f66c42e869c0534fbe8f89e3826d53c';
    const sha = (b: Uint8Array) => createHash('sha256').update(masked(b)).digest('hex');
    expect(sha(build())).toBe(BEFORE);
    expect(sha(build([]))).toBe(BEFORE);
  });
});

describe('the MOBI cover, EXTH 201', () => {
  const PICTURES = [png(1, 2, 3), png(4, 5), png(6)];
  const build = (coverIndex?: number, images: Uint8Array[] = PICTURES) =>
    buildMobi({ title: 'Test Script', author: 'Jane Doe', html: sampleHtml(), images, coverIndex });
  const u32of = (data: Uint8Array | undefined) => (data && data.length === 4 ? u32(data, 0) : undefined);

  test('names the cover by its offset from the first image record, 0-based, in four bytes', () => {
    for (const coverIndex of [0, 1, 2]) {
      const mobi = build(coverIndex);
      const data = exth(mobi, 201);
      expect([...(data ?? [])]).toEqual([0, 0, 0, coverIndex]);
      // The record it lands on is that picture, counted the way a reader
      // counts: from the header's first image record.
      expect(records(mobi)[header(mobi).firstImage + u32of(data)!]).toEqual(PICTURES[coverIndex]);
    }
  });

  test('without a cover there is no EXTH 201, with pictures or without', () => {
    expect(exth(build(), 201)).toBeUndefined();
    expect(exth(build(undefined, []), 201)).toBeUndefined();
  });

  test('the EXTH block stays whole around it: its length, its count, its padding, the full name after it', () => {
    const without = build();
    const withCover = build(1);
    const r0 = (b: Uint8Array) => recordOffsets(b)[0];
    const exthAt = (b: Uint8Array) => r0(b) + 16 + u32(b, r0(b) + 20);
    // The MOBI header itself does not move: same length, EXTH flag still set.
    expect(u32(withCover, r0(withCover) + 20)).toBe(232);
    expect(u32(withCover, r0(withCover) + 128) & 0x40).toBe(0x40);
    expect(exthAt(withCover)).toBe(exthAt(without));
    // One record more, twelve bytes more (an 8-byte head and a u32), and the
    // walk ends exactly where the block's own length says (exthRecords
    // throws otherwise).
    expect(u32(withCover, exthAt(withCover) + 8)).toBe(u32(without, exthAt(without) + 8) + 1);
    expect(u32(withCover, exthAt(withCover) + 4)).toBe(u32(without, exthAt(without) + 4) + 12);
    // In type order, and every other record exactly as it was.
    const types = (b: Uint8Array) => exthRecords(b).map((r) => r.type);
    expect(types(without)).toEqual([100, 501, 503, 524]);
    expect(types(withCover)).toEqual([100, 201, 501, 503, 524]);
    expect(exthRecords(withCover).filter((r) => r.type !== 201)).toEqual(exthRecords(without));
    // 84, the full name's offset, moves down by the same twelve bytes, still
    // on a 4-byte boundary past the padded block, and still finds the title,
    // whose length (88) does not change.
    const nameAt = (b: Uint8Array) => u32(b, r0(b) + 84);
    expect(nameAt(withCover)).toBe(nameAt(without) + 12);
    expect((nameAt(withCover) - 248) % 4).toBe(0);
    expect(nameAt(withCover) - 248).toBeGreaterThanOrEqual(u32(withCover, exthAt(withCover) + 4));
    expect(u32(withCover, r0(withCover) + 88)).toBe(u32(without, r0(without) + 88));
    expect(ascii(withCover, r0(withCover) + nameAt(withCover), 11)).toBe('Test Script');
    // Record 0 grows by those twelve bytes and stays a multiple of four.
    const rec0 = (b: Uint8Array) => records(b)[0];
    expect(rec0(withCover).length).toBe(rec0(without).length + 12);
    expect(rec0(withCover).length % 4).toBe(0);
  });

  test('every other record, and every header field that names one, is the book without a cover', () => {
    const without = build();
    const withCover = build(2);
    expect(records(withCover).slice(1)).toEqual(records(without).slice(1));
    expect(header(withCover)).toEqual(header(without));
    // Record 0 is the book without a cover with the one record added: take
    // those twelve bytes back out, restore the three fields that count them,
    // and the two are the same bytes.
    const r0 = records(withCover)[0];
    const at = 248 + 12 + exthRecords(without).filter((r) => r.type === 100)[0].data.length + 8;
    const back = new Uint8Array([...r0.slice(0, at), ...r0.slice(at + 12)]);
    const put = (o: number, v: number) => back.set([v >>> 24, (v >>> 16) & 0xff, (v >>> 8) & 0xff, v & 0xff], o);
    put(84, u32(r0, 84) - 12);
    put(248 + 4, u32(r0, 248 + 4) - 12);
    put(248 + 8, u32(r0, 248 + 8) - 1);
    expect([...back]).toEqual([...records(without)[0]]);
  });

  test('a cover index the pictures cannot answer is a caller bug, and says so', () => {
    // EXTH 201 counts from the first image record, so one past the last
    // picture names FLIS, and a reader would take a trailer for the cover.
    expect(() => build(3)).toThrow(/cover/);
    expect(() => build(-1)).toThrow(/cover/);
    expect(() => build(0.5)).toThrow(/cover/);
    expect(() => build(0, [])).toThrow(/cover/);
    // The control: the last picture builds.
    expect(() => build(2)).not.toThrow();
  });

  withCalibre(`Calibre takes a JPEG cover from the record EXTH 201 names, byte for byte${NO_CALIBRE}`, async () => {
    // Two JPEGs, both on the page. Calibre's reader, given no EXTH 201, makes
    // the first image record the cover if it is a JPEG, so a cover of 0 would
    // pass with the record unread; the cover here is the SECOND picture, and
    // the book without the record is the control that shows the difference
    // is 201's.
    const pictures = [greyJpeg(1), greyJpeg(2)];
    const html = '<html><head></head><body><p>Before.</p>\n'
      + '<p align="center"><img recindex="00001" alt="Illustration"/></p>\n'
      + '<p align="center"><img recindex="00002" alt="Illustration"/></p>\n<p>After.</p></body></html>';
    const make = (coverIndex?: number) => buildMobi({ title: 'Cover Probe', html, images: pictures, coverIndex });

    const named = await throughCalibre(make(1), 'jpeg-cover');
    expect(await named.cover()).toEqual(pictures[1]);
    expect(await named.shown()).toEqual(pictures);

    const unnamed = await throughCalibre(make(), 'jpeg-no-cover');
    expect(await unnamed.cover()).toEqual(pictures[0]);
    expect(await unnamed.shown()).toEqual(pictures);
  }, 180000);
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
    return { html, art, mobi: buildMobi({ ...meta, html, images, coverIndex: art.coverIndex }) };
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

  withCalibre(`Calibre reads every picture back, byte for byte, in order, each from the right <img>${NO_CALIBRE}`, async () => {
    const { mobi, art } = book();
    // The book renderBooks builds, with its title card named as the cover.
    // The card is a PNG, and calibre's reader looks a cover up only under a
    // .jpg name, so it takes no cover from this book at all: no cover meta,
    // no cover item. Which is why the card stays on its page, and here it
    // is, first in the flow.
    expect(art.coverIndex).toBe(0);
    expect([...(exth(mobi, 201) ?? [])]).toEqual([0, 0, 0, 0]);
    const back = await throughCalibre(mobi, 'images');
    expect(await back.shown()).toEqual(inScriptOrder());
    expect(back.coverMeta().id).toBeUndefined();
    expect(await back.cover()).toBeUndefined();
  }, 120000);
});
