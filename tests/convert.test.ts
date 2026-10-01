import { afterEach, describe, test, expect, spyOn } from 'bun:test';
import { existsSync } from 'node:fs';
import JSZip from 'jszip';
import { Fountain } from 'fountain-js';
import { tokensToBody, tokensToPreviewHtml } from '../src/epub/html';
import { buildEpub } from '../src/epub/build';
import { screenplayCss } from '../src/epub/css';
import { resolveFormatOptions } from '../src/options';
import {
  convertPdf,
  convertFountain,
  imageWarnings,
  ScannedPdfError,
  NotAScreenplayError,
} from '../src/convert';
import { extractDocument } from '../src/parser/extract';
// Namespaces, not named imports, because the spies below replace a binding
// on the module itself: that is what the code under test calls through.
import * as pictures from '../src/parser/images';
import * as png from '../src/parser/png';

// Committed, invented, and safe to publish — see tools/make-fixture.py.
const PUBLIC = new URL('./fixtures/', import.meta.url).pathname;
// Real scripts are confidential and gitignored; they add coverage locally
// but nothing here may assert anything that identifies one.
const PRIVATE = new URL('../fixtures/', import.meta.url).pathname;
const hasPrivate = existsSync(`${PRIVATE}final-draft.pdf`);

async function bytes(path: string): Promise<Uint8Array> {
  return new Uint8Array(await Bun.file(path).arrayBuffer());
}

describe('convertPdf', () => {
  test('screenplay PDF converts end-to-end', async () => {
    const result = await convertPdf(await bytes(`${PUBLIC}screenplay.pdf`));
    expect(result.meta.title).toBe('The Last Video Store');
    expect(result.meta.author).toBe('A. N. Placeholder');
    expect(result.screenplay!.scenes.length).toBe(5);
    expect(result.fountainText).toContain('INT. THE LAST VIDEO STORE - NIGHT');
    // EPUB is a valid zip with one chapter per scene (+ Opening if any)
    const zip = await JSZip.loadAsync(result.epub);
    const opf = await zip.file('OEBPS/package.opf')!.async('string');
    expect(opf).toContain('<dc:title>The Last Video Store</dc:title>');
  }, 60000);

  test('image-only PDF raises ScannedPdfError', async () => {
    // The await is load-bearing: unawaited, .rejects resolves after the
    // test body and the assertion can never fail the test.
    await expect(convertPdf(await bytes(`${PUBLIC}blank-pages.pdf`)))
      .rejects.toThrow(ScannedPdfError);
  }, 60000);

  test('non-screenplay PDF raises NotAScreenplayError', async () => {
    await expect(convertPdf(await bytes(`${PUBLIC}prose.pdf`)))
      .rejects.toThrow(NotAScreenplayError);
  }, 60000);

  test('rebuilding the same script keeps the same dc:identifier', async () => {
    // The app rewrites the library EPUB on every options change; a fresh
    // urn:uuid each render would make readers treat the same book as a
    // new one (and re-index it) every time. Identity follows CONTENT.
    const fountain = 'Title: Stable Book\n\nINT. ROOM - DAY\n\nAction holds.\n';
    const idOf = async (epub: Uint8Array) => {
      const zip = await JSZip.loadAsync(epub);
      const opf = await zip.file('OEBPS/package.opf')!.async('string');
      return opf.match(/<dc:identifier[^>]*>([^<]+)</)![1];
    };
    const a = await convertFountain(fountain);
    const b = await convertFountain(fountain);
    expect(await idOf(a.epub)).toBe(await idOf(b.epub));

    const c = await convertFountain(fountain + '\nMore action arrives.\n');
    expect(await idOf(c.epub)).not.toBe(await idOf(a.epub));
  });

  test('title/author overrides win over detection', async () => {
    const result = await convertPdf(await bytes(`${PUBLIC}screenplay.pdf`), {
      title: 'Override Title',
      author: 'Override Author',
    });
    expect(result.meta.title).toBe('Override Title');
    expect(result.meta.author).toBe('Override Author');
  }, 60000);
});

// Shape only — a feature-length script exercises paths the 5-page fixture
// cannot (page-break interruptions, CONT'D runs, a deep character roster).
describe.skipIf(!hasPrivate)('convertPdf on a full-length script', () => {
  test('converts with a plausible scene and character count', async () => {
    const result = await convertPdf(await bytes(`${PRIVATE}final-draft.pdf`));
    expect(result.screenplay!.scenes.length).toBeGreaterThan(50);
    expect(result.meta.title).not.toBe('Untitled Screenplay');
    expect(result.warnings).toEqual([]);
  }, 60000);
});

describe('convertFountain', () => {
  const SRC = `Title: Direct Input\nAuthor: Someone\n\nINT. VOID - DAY\n\nText appears.\n\n@VOICE\nHello.\n`;

  test('fountain text converts to EPUB with metadata from title block', async () => {
    const result = await convertFountain(SRC);
    expect(result.meta.title).toBe('Direct Input');
    expect(result.meta.author).toBe('Someone');
    const zip = await JSZip.loadAsync(result.epub);
    const ch = await zip.file('OEBPS/text/body001.xhtml')!.async('string');
    expect(ch).toContain('INT. VOID - DAY');
    expect(ch).toContain('<p class="character">VOICE</p>');
  });

  test('untitled fountain falls back to a default title', async () => {
    const result = await convertFountain('INT. SOMEWHERE - DAY\n\nAction.\n');
    expect(result.meta.title).toBe('Untitled Screenplay');
  });
});

// contdMode is consumed in fountain/serialize.ts, upstream of the .fountain
// cache boundary — so on Fountain input it is already baked into the cue text
// and asking to strip it cannot work. It used to fail silently, which is worse
// than failing: the app's reader re-renders from the cached .fountain, so the
// control looked dead exactly where a user would reach for it.
describe('convertFountain warns when a stage-1 option cannot apply', () => {
  const WITH_CONTD =
    'INT. STORE - NIGHT\n\nAction.\n\n@MARGO\nFirst half.\n\n@MARGO (CONT’D)\nSecond half.\n';
  const WITHOUT_CONTD = 'INT. STORE - NIGHT\n\nAction.\n\n@MARGO\nOnly speech.\n';

  test('strip requested but (CONT’D) is already baked into the cues', async () => {
    const result = await convertFountain(WITH_CONTD, { format: { contdMode: 'strip' } });
    expect(result.warnings).toHaveLength(1);
    expect(result.warnings[0]).toContain("(CONT'D)");
    // Must name the fix, not just the failure.
    expect(result.warnings[0]).toMatch(/re-?convert/i);
  });

  test('the cues really do keep their (CONT’D), which is what the warning is about', async () => {
    const result = await convertFountain(WITH_CONTD, { format: { contdMode: 'strip' } });
    const zip = await JSZip.loadAsync(result.epub);
    const ch = await zip.file('OEBPS/text/body001.xhtml')!.async('string');
    expect(ch).toContain('CONT');
  });

  test('silent when there is no (CONT’D) to strip', async () => {
    const result = await convertFountain(WITHOUT_CONTD, { format: { contdMode: 'strip' } });
    expect(result.warnings).toEqual([]);
  });

  test('silent for keep and auto, which do not promise removal here', async () => {
    for (const contdMode of ['keep', 'auto'] as const) {
      const result = await convertFountain(WITH_CONTD, { format: { contdMode } });
      expect(result.warnings).toEqual([]);
    }
  });

  test('PDF input never warns — there the option genuinely applies', async () => {
    const result = await convertPdf(await bytes(`${PUBLIC}screenplay.pdf`), {
      format: { contdMode: 'strip' },
    });
    expect(result.warnings.filter((w) => w.includes("CONT'D"))).toEqual([]);
    const zip = await JSZip.loadAsync(result.epub);
    const ch = await zip.file('OEBPS/text/body001.xhtml')!.async('string');
    expect(ch).not.toContain('CONT&apos;D');
  }, 60000);
});

describe('convertFountain previewHtml', () => {
  test('returns the preview document', async () => {
    const src = 'Title: T\n\nINT. LAB - DAY\n\nBeakers bubble.\n\nELI\nEureka.\n';
    const result = await convertFountain(src, {});
    expect(result.previewHtml).toContain('class="dialogue-block"');
    expect(result.previewHtml).toContain('<style>');
  });
});

// The pixels leave convert.ts NAMED and unwritten. Everything about this
// describe is about that line: convert.ts does no filesystem work and the
// CLI owns disk, so the test for "the folder appears" is in cli.test.ts and
// the test for "the bytes are here at all" is this one.
const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/** The contents' entries, as [match, anchor, title]. */
async function tocOf(zip: JSZip) {
  const nav = await zip.file('OEBPS/nav.xhtml')!.async('string');
  const toc = nav.slice(nav.indexOf('epub:type="toc"'), nav.indexOf('</nav>'));
  return [...toc.matchAll(/<a href="[^"#]*#(sc-\d+)">([^<]*)<\/a>/g)];
}

/** Every body file, in reading order, as one string. */
async function bodyOf(zip: JSZip) {
  const names = Object.keys(zip.files).filter((n) => /^OEBPS\/text\/body\d+\.xhtml$/.test(n)).sort();
  return (await Promise.all(names.map((n) => zip.file(n)!.async('string')))).join('');
}

// Files only: JSZip lists a folder entry for each directory a path implies.
const imageEntries = (zip: JSZip) =>
  Object.keys(zip.files).filter((n) => n.startsWith('OEBPS/images/') && !zip.files[n].dir).sort();

describe('convertPdf carries the pictures', () => {
  test('both kept pictures come back as named PNG bytes', async () => {
    const result = await convertPdf(await bytes(`${PUBLIC}images.pdf`));
    expect(result.images.map((i) => i.name)).toEqual(['p001-01.png', 'p002-01.png']);
    for (const img of result.images) {
      expect([...img.bytes.slice(0, 8)]).toEqual(PNG_MAGIC);
      expect(img.bytes.length).toBeGreaterThan(100);
    }
  }, 60000);

  test('every name carried is a name the .fountain asks for', async () => {
    // The join the whole feature rests on: the note names a file and the
    // picture folder has to hold that exact name. Nothing pairs them
    // positionally, so a renumbering on either side shows up here.
    const result = await convertPdf(await bytes(`${PUBLIC}images.pdf`));
    expect(result.images.length).toBeGreaterThan(0);
    for (const img of result.images) {
      expect(result.fountainText).toContain(`[[img: ${img.name}`);
    }
  }, 60000);

  test('a script with no pictures returns an empty array and says nothing', async () => {
    const result = await convertPdf(await bytes(`${PUBLIC}screenplay.pdf`));
    expect(result.images).toEqual([]);
    expect(result.warnings).toEqual([]);
  }, 60000);

  test('the title card opens an Opening that holds its figure, and the scenes follow it', async () => {
    // images.pdf's title card is a note ABOVE the first heading in the
    // .fountain. The conversion hands the renderer the pictures it carried,
    // so that note is a figure: content, which opens a section of its own
    // ("Opening") the way any line above the first heading does, and every
    // heading's anchor sits one along from the script's own count.
    const result = await convertPdf(await bytes(`${PUBLIC}images.pdf`));
    expect(result.fountainText.indexOf('[[img: p001-01.png')).toBeLessThan(
      result.fountainText.indexOf('INT. ARCHIVE ROOM - DAY'),
    );
    const zip = await JSZip.loadAsync(result.epub);
    const entries = await tocOf(zip);
    expect(entries.map((m) => m[2])).toEqual([
      'Opening', ...result.screenplay!.scenes.map((s) => s.heading),
    ]);
    expect(entries[1][1]).toBe('sc-002');
    const body = await bodyOf(zip);
    expect(body).toMatch(
      /<section class="scene" id="sc-001">\s*<p class="figure w100 figure-full"><img src="\.\.\/images\/p001-01\.png" alt="Illustration"\/><\/p>\s*<\/section>/,
    );
    expect(body).toContain('<p class="figure w40"><img src="../images/p002-01.png" alt="Illustration"/></p>');
    // And the book carries what its pages point at: the bytes the conversion
    // carried, under the names the notes use, and nothing else.
    expect(imageEntries(zip)).toEqual(['OEBPS/images/p001-01.png', 'OEBPS/images/p002-01.png']);
    for (const img of result.images) {
      expect([...await zip.file(`OEBPS/images/${img.name}`)!.async('uint8array')]).toEqual([...img.bytes]);
    }
  }, 60000);

  test('with includeImages off, the book is the one a script without pictures gets', async () => {
    // The notes render nothing, so the title card's opens nothing either: an
    // empty section would be an "Opening" in the contents, a blank page under
    // scenePageBreaks, and every scene anchor one along. Compared with the
    // renderer's own answer for the same text with no pictures in hand, so
    // "nothing" is checked rather than described.
    const format = resolveFormatOptions({ includeImages: false });
    const result = await convertPdf(await bytes(`${PUBLIC}images.pdf`), { format: { includeImages: false } });
    // Still carried: the option is about the book, and the folder is written
    // either way, so turning it back on needs no re-conversion.
    expect(result.images.map((i) => i.name)).toEqual(['p001-01.png', 'p002-01.png']);
    const zip = await JSZip.loadAsync(result.epub);
    const tokens = new Fountain().parse(result.fountainText, true).tokens;
    expect(await bodyOf(zip)).toBe(tokensToBody(tokens, { format }).files.map((f) => f.xhtml).join(''));
    const entries = await tocOf(zip);
    expect(entries.map((m) => m[2])).toEqual(result.screenplay!.scenes.map((s) => s.heading));
    expect(entries[0][1]).toBe('sc-001');
    expect(imageEntries(zip)).toEqual([]);
    expect(await zip.file('OEBPS/package.opf')!.async('string')).not.toContain('image/png');
    expect(await zip.file('OEBPS/style.css')!.async('string')).toBe(screenplayCss(format));
  }, 60000);

  test('a script without pictures gets the book it got before pictures could reach one', async () => {
    // Every entry, in order and byte for byte, against the render that hands
    // the renderer no pictures at all, which is the whole of what this
    // pipeline did before it carried them. The one line two runs of the same
    // conversion disagree on is the dcterms:modified stamp, so it is the one
    // line set aside.
    const result = await convertPdf(await bytes(`${PUBLIC}screenplay.pdf`));
    const format = resolveFormatOptions();
    const got = await JSZip.loadAsync(result.epub);
    const opf = await got.file('OEBPS/package.opf')!.async('string');
    const identifier = opf.match(/<dc:identifier[^>]*>([^<]+)</)![1];
    const tokens = new Fountain().parse(result.fountainText, true).tokens;
    const want = await JSZip.loadAsync(
      await buildEpub({ identifier, ...result.meta }, tokensToBody(tokens, { format }), format),
    );
    const strip = (s: string) => s.replace(/<meta property="dcterms:modified">[^<]*<\/meta>/, '');
    expect(Object.keys(got.files)).toEqual(Object.keys(want.files));
    for (const name of Object.keys(want.files)) {
      if (want.files[name].dir) continue;
      expect(strip(await got.file(name)!.async('string')), name)
        .toBe(strip(await want.file(name)!.async('string')));
    }
  }, 60000);

  test('a healthy picture document warns about nothing', async () => {
    // The two warnings below are exceptional by design. If either fired on a
    // document whose pictures all arrived, it would fire on every conversion,
    // which is how a warning stops being read.
    const result = await convertPdf(await bytes(`${PUBLIC}images.pdf`));
    expect(result.warnings).toEqual([]);
  }, 60000);

  test('a picture pdf.js cannot decode is reported, not silently missing', async () => {
    // The failure the design is most afraid of, made real: a script that
    // loses its art with no error reads exactly like one that never had
    // pictures. Clobbering an image XObject's payload IN PLACE keeps every
    // byte offset in the file valid, so this is a fixture with one broken
    // picture rather than a broken fixture.
    const pdf = await bytes(`${PUBLIC}images.pdf`);
    const text = new TextDecoder('latin1').decode(pdf);
    const at = text.search(/\/Subtype\s*\/Image/);
    const from = text.indexOf('stream', at) + 'stream\n'.length;
    const to = text.indexOf('endstream', from);
    // Asserted, not assumed: a regenerated fixture that compresses its object
    // headers has to fail HERE rather than quietly test nothing.
    expect(at).toBeGreaterThan(0);
    expect(to - from).toBeGreaterThan(100);
    pdf.fill(0x41, from + 4, to - 4);

    const result = await convertPdf(pdf);
    // The title card is gone (it is the picture that was broken) and the
    // panel on sheet 2 is untouched: filenames are per page, so losing one
    // does not renumber the other.
    expect(result.images.map((i) => i.name)).toEqual(['p002-01.png']);
    expect(result.warnings).toEqual([
      '1 picture could not be read out of the PDF, so it was left out.',
    ]);
  }, 60000);

  test('the scanned guard fires before any picture is carried', async () => {
    // Not a statement about disk (nothing here writes), but about ORDER:
    // ScannedPdfError has to leave through the throw, never through a result
    // carrying full-page PNGs for the caller to write down.
    await expect(convertPdf(await bytes(`${PUBLIC}blank-pages.pdf`)))
      .rejects.toThrow(ScannedPdfError);
  }, 60000);

  test('Fountain input carries nothing out', async () => {
    // Its pictures already sit in the folder beside it; there is no PDF to
    // carry them out of, so there is nothing for a caller to write. That
    // holds when it is handed the folder's pictures to render, too: those
    // came from the folder, and writing them back would be a copy onto
    // themselves.
    const src = 'Title: T\n\nINT. A - DAY\n\n[[img: p001-01.png w=71]]\n';
    expect((await convertFountain(src)).images).toEqual([]);
    const plate = png.encodePng(new Uint8Array([0, 0, 0]), 1, 1);
    expect((await convertFountain(src, { images: new Map([['p001-01.png', plate]]) })).images).toEqual([]);
  });
});

// The re-render's half: a `.fountain` and the pictures read from the folder
// beside it (src/library-images.ts does the reading; convert.ts never touches
// a disk). What reaches the book is decided by the map it is handed, filtered
// again here because any caller may hand one over.
describe('convertFountain renders the pictures it is handed', () => {
  const SRC = 'Title: T\n\nINT. LAB - DAY\n\nBefore.\n\n[[img: p001-01.png w=40]]\n\nAfter.\n';
  const FIGURE = '<p class="figure w40"><img src="../images/p001-01.png" alt="Illustration"/></p>';
  const plate = () => png.encodePng(new Uint8Array([200, 10, 10]), 1, 1);

  test('a note whose picture is in the map is a figure, and the book carries the picture', async () => {
    const bytes = plate();
    const zip = await JSZip.loadAsync(
      (await convertFountain(SRC, { images: new Map([['p001-01.png', bytes]]) })).epub,
    );
    expect(await bodyOf(zip)).toContain(FIGURE);
    expect(imageEntries(zip)).toEqual(['OEBPS/images/p001-01.png']);
    expect([...await zip.file('OEBPS/images/p001-01.png')!.async('uint8array')]).toEqual([...bytes]);
  });

  test('a picture the map holds and no note names stays out of the book', async () => {
    const zip = await JSZip.loadAsync((await convertFountain(SRC, {
      images: new Map([['p001-01.png', plate()], ['p009-01.png', plate()]]),
    })).epub);
    expect(imageEntries(zip)).toEqual(['OEBPS/images/p001-01.png']);
  });

  test('bytes that are not a PNG are no picture, whatever the name says', async () => {
    // A JPEG renamed `.png` would be declared image/png in the manifest and
    // fail epubcheck (OPF-029, PKG-022). The folder's reader already refuses
    // one, and counts it; this is the same refusal for a map from anywhere
    // else, so the note renders nothing rather than a book that fails.
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46, 0x49, 0x46, 0, 1]);
    const zip = await JSZip.loadAsync(
      (await convertFountain(SRC, { images: new Map([['p001-01.png', jpeg]]) })).epub,
    );
    expect(await bodyOf(zip)).not.toContain('class="figure');
    expect(imageEntries(zip)).toEqual([]);
  });

  test('with includeImages off, the map changes nothing', async () => {
    const off = { includeImages: false };
    const without = await JSZip.loadAsync((await convertFountain(SRC, { format: off })).epub);
    const withMap = await JSZip.loadAsync((await convertFountain(SRC, {
      format: off, images: new Map([['p001-01.png', plate()]]),
    })).epub);
    expect(await bodyOf(withMap)).toBe(await bodyOf(without));
    expect(await bodyOf(withMap)).not.toContain('class="figure');
    expect(imageEntries(withMap)).toEqual([]);
  });
});

// The reader preview is rendered from the same pictures as the book, so what
// the window shows is what the book shows. What the preview does with them is
// epub-images.test.ts's subject; these are about the pictures reaching it.
describe('the preview is handed the pictures the book is', () => {
  const pictureMap = (images: { name: string; bytes: Uint8Array }[]) =>
    new Map(images.map((img) => [img.name, img.bytes]));
  const dataSrcs = (html: string) => (html.match(/<img src="data:image\/png;base64,/g) ?? []).length;

  test('a PDF conversion’s preview shows the pictures it carried', async () => {
    const result = await convertPdf(await bytes(`${PUBLIC}images.pdf`));
    const tokens = new Fountain().parse(result.fountainText, true).tokens;
    expect(dataSrcs(result.previewHtml)).toBe(2);
    expect(result.previewHtml)
      .toBe(tokensToPreviewHtml(tokens, resolveFormatOptions(), pictureMap(result.images)));
  }, 60000);

  test('with includeImages off, a PDF conversion’s preview shows none', async () => {
    const format = resolveFormatOptions({ includeImages: false });
    const result = await convertPdf(await bytes(`${PUBLIC}images.pdf`), { format: { includeImages: false } });
    const tokens = new Fountain().parse(result.fountainText, true).tokens;
    expect(dataSrcs(result.previewHtml)).toBe(0);
    expect(result.previewHtml).toBe(tokensToPreviewHtml(tokens, format));
  }, 60000);

  test('a script without pictures gets the preview it got before', async () => {
    const result = await convertPdf(await bytes(`${PUBLIC}screenplay.pdf`));
    const tokens = new Fountain().parse(result.fountainText, true).tokens;
    expect(result.previewHtml).toBe(tokensToPreviewHtml(tokens, resolveFormatOptions()));
  }, 60000);

  test('a .fountain’s preview shows the pictures it is handed, after the same filter as the book', async () => {
    const src = 'Title: T\n\nINT. LAB - DAY\n\nBefore.\n\n[[img: p001-01.png w=40]]\n\n'
      + 'Between.\n\n[[img: p002-01.png w=60]]\n\nAfter.\n';
    const plate = png.encodePng(new Uint8Array([200, 10, 10]), 1, 1);
    // The second is a JPEG under a PNG's name: no picture to the book, and so
    // none to the preview either.
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46, 0x49, 0x46, 0, 1]);
    const { previewHtml } = await convertFountain(src, {
      images: new Map([['p001-01.png', plate], ['p002-01.png', jpeg]]),
    });
    expect(previewHtml).toContain(
      `<p class="figure w40"><img src="data:image/png;base64,${Buffer.from(plate).toString('base64')}" alt="Illustration"/></p>`,
    );
    expect(dataSrcs(previewHtml)).toBe(1);
  });
});

// Both refusals are decided by the TEXT, so a document either guard turns away
// must not pay for its pictures first. Decoding them ahead of the guards
// costs a scan on the local corpus 3.7s to refuse instead of 0.09s. Counted
// with spies rather than timed, because a clock in a unit test measures the
// machine. The two spies are the expensive halves of the work: waiting for a
// picture's pixels, and deflating them into a PNG.
//
// Neither committed refusal fixture can show this: blank-pages.pdf and
// prose.pdf carry no pictures, so they cost nothing either way. The cases are
// images.pdf with its text taken away (a scan: every picture, no words) and
// with its words disguised (a document with words and pictures but no scene
// and no speaker), each edited IN PLACE so every byte offset stays valid, the
// way the broken-picture test above builds its document.
describe('a refused document does not pay for its pictures', () => {
  const spies: { mockRestore(): void }[] = [];
  afterEach(() => {
    for (const s of spies.splice(0)) s.mockRestore();
  });
  const watch = () => {
    const resolves = spyOn(pictures, 'resolveImageData');
    const encodes = spyOn(png, 'encodePng');
    spies.push(resolves, encodes);
    return () => ({ resolves: resolves.mock.calls.length, encodes: encodes.mock.calls.length });
  };

  /**
   * images.pdf with every text block run through `edit`, byte for byte.
   *
   * The pattern is the exact shape tools/make-fixture.py writes a page's text
   * in, and the count is asserted rather than assumed: a regenerated fixture
   * that writes its text differently has to fail HERE, not quietly hand the
   * tests below an unedited screenplay.
   */
  async function edited(edit: (pdf: Uint8Array, from: number, to: number) => void) {
    const pdf = await bytes(`${PUBLIC}images.pdf`);
    // windows-1252, which is what 'latin1' means to TextDecoder: one UTF-16
    // unit per byte, so a string index IS a byte offset. Only ever used to
    // find places; the bytes are edited in the original array.
    const text = new TextDecoder('latin1').decode(pdf);
    const blocks = [...text.matchAll(/\nBT\n\/F1 12 Tf\n[\s\S]*?\nET\n/g)];
    expect(blocks).toHaveLength(5);
    for (const b of blocks) edit(pdf, b.index! + 1, b.index! + b[0].length - 1);
    return pdf;
  }

  /** Every word gone: spaces in place of each text block. */
  const scan = () => edited((pdf, from, to) => pdf.fill(0x20, from, to));

  /** Every capital in every string turned to `x`, which leaves nothing a
   * scene heading or a cue could be made of while keeping every line. */
  const disguised = () =>
    edited((pdf, from, to) => {
      let inString = false;
      for (let i = from; i < to; i++) {
        const c = pdf[i];
        if (inString && c === 0x5c) {
          i++; // an escape: whatever follows is not the string's end
          continue;
        }
        if (c === 0x28) inString = true;
        else if (c === 0x29) inString = false;
        else if (inString && c >= 0x41 && c <= 0x5a) pdf[i] = 0x78;
      }
    });

  test('the spies see the work when a document is converted', async () => {
    // The control, without which the zeros below prove nothing: a spy that
    // missed the calls would read 0 on every document.
    const counts = watch();
    const result = await convertPdf(await bytes(`${PUBLIC}images.pdf`));
    expect(result.images.length).toBe(2);
    expect(counts().resolves).toBeGreaterThan(0);
    expect(counts().encodes).toBeGreaterThan(0);
  }, 60000);

  test('a scan is refused without resolving or encoding a single picture', async () => {
    const counts = watch();
    await expect(convertPdf(await scan())).rejects.toThrow(ScannedPdfError);
    expect(counts()).toEqual({ resolves: 0, encodes: 0 });
  }, 60000);

  test('a document that is not a screenplay is refused the same way', async () => {
    const counts = watch();
    await expect(convertPdf(await disguised())).rejects.toThrow(NotAScreenplayError);
    expect(counts()).toEqual({ resolves: 0, encodes: 0 });
  }, 60000);

  test('both edited documents still carry the pictures they did not pay for', async () => {
    // So the zeros above are savings and not an absence: read in full, the
    // scan keeps pictures (more than images.pdf does, since a sheet with no
    // text cannot have a watermark behind it), and the disguised document
    // converted with --force, which overrules the one guard that can be
    // overruled, carries the same two the original does.
    expect((await extractDocument(await scan())).images.length).toBeGreaterThan(0);
    const forced = await convertPdf(await disguised(), { force: true });
    expect(forced.images.map((i) => i.name)).toEqual(['p001-01.png', 'p002-01.png']);
  }, 60000);
});

// The wiring, which no committed fixture can reach: a document has to carry
// more than MAX_IMAGES pictures before convertPdf has anything to say. The
// local corpus has exactly one, and it is the only place the count that
// reaches `imageWarnings` is the count a real extraction produced rather than
// one a test wrote down.
describe.skipIf(!existsSync(`${PRIVATE}pitchdeck.pdf`))('the cap warning, wired up', () => {
  test('a document over the cap says how many it has and how many it kept', async () => {
    const result = await convertPdf(await bytes(`${PRIVATE}pitchdeck.pdf`), { force: true });
    expect(result.images.length).toBe(60);
    expect(result.warnings.some((w) => /Only the first 60 were kept\./.test(w))).toBe(true);
  }, 120000);
});

describe('the image warnings', () => {
  // Exported and tested directly because no clean fixture can produce either
  // input: `overCap` needs a 61-picture document and `failed` needs an XObject
  // pdf.js cannot decode. The wording is the deliverable (both counts exist to
  // be said out loud to a filmmaker), so it is pinned here rather than left to
  // whichever document happens to trip it.
  const stats = (over: number, failed: number) => ({
    overCap: over,
    failed,
    droppedBy: { pixels: 0, width: 0, height: 0, area: 0, furniture: 0, background: 0 },
  });
  /** The pictures that DID make it, which is all this needs of them. */
  const carried = (n: number) =>
    Array.from({ length: n }, (_, i) => ({ name: `p00${i}-01.png`, bytes: new Uint8Array(1) }));

  test('silence when every picture was carried', () => {
    expect(imageWarnings(stats(0, 0), carried(12))).toEqual([]);
  });

  test('the cap says how many were found and how many made it', () => {
    const [w] = imageWarnings(stats(13, 0), carried(60));
    expect(w).toContain('73');
    expect(w).toContain('60');
    expect(w).not.toMatch(/overCap|MAX_IMAGES|undefined|NaN/);
  });

  test('a lost picture is reported even when others were kept', () => {
    const [w] = imageWarnings(stats(0, 3), carried(5));
    expect(w).toContain('3');
    expect(w).toMatch(/pictures/);
  });

  test('one lost picture is singular', () => {
    expect(imageWarnings(stats(0, 1), [])[0]).toMatch(/1 picture (?!s)/);
  });

  test('both counts are reported, cap first', () => {
    const ws = imageWarnings(stats(2, 2), carried(60));
    expect(ws).toHaveLength(2);
    expect(ws[0]).toContain('62');
  });

  test('no em dashes, and nothing a reader has to be a programmer to parse', () => {
    for (const w of [...imageWarnings(stats(9, 9), carried(60))]) {
      expect(w).not.toContain('—');
      expect(w).toMatch(/^[A-Z0-9]/);
      expect(w.endsWith('.')).toBe(true);
    }
  });

  // `decodersUnavailable` is the one cause of a lost picture that is OURS
  // rather than the file's: pdf.js loads its JPEG2000 and JBIG2 decoders from a
  // directory we have to write at startup (src/parser/pdfjs-wasm.ts), and if
  // that write fails every picture in those formats fails at once. The count
  // alone would send a filmmaker off to re-export a script that was fine.
  const undecodable = (failed: number) => ({ ...stats(0, failed), decodersUnavailable: true });

  test('a decoder failure says so, beside the count and not instead of it', () => {
    const ws = imageWarnings(undecodable(34), []);
    expect(ws).toHaveLength(2);
    expect(ws[0]).toContain('34');
    expect(ws[1]).toMatch(/JPEG2000/);
  });

  test('and says nothing at all when no picture was actually lost', () => {
    // A machine that cannot write a temp directory still converts a script
    // with no JPEG2000 in it perfectly, which is nearly every script. Warning
    // there is how a warning stops being read.
    expect(imageWarnings(undecodable(0), carried(3))).toEqual([]);
  });

  test('the cause sentence is plain English too', () => {
    const [, cause] = imageWarnings(undecodable(2), []);
    expect(cause).not.toContain('—');
    expect(cause).not.toMatch(/wasm|wasmUrl|pdf\.js|decodersUnavailable/);
    expect(cause.endsWith('.')).toBe(true);
  });
});
