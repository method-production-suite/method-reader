// EPUB3 packaging: OCF zip with mimetype first (stored), package.opf,
// nav.xhtml TOC from scene chapters, generated title page, and the pictures
// the body shows (stored, registry #19).
import JSZip from 'jszip';
import type { BookBody } from './html';
import { escapeXml } from './html';
import { screenplayCss } from './css';
import type { FormatOptions } from '../options';
import { DEFAULT_FORMAT_OPTIONS } from '../options';

export interface BookMeta {
  title: string;
  author?: string;
  language?: string;
  /** stable identifier; defaults to a random urn:uuid */
  identifier?: string;
}

/**
 * One picture on its way into a book: the bytes, and the name the
 * `.fountain` note and the body's `<img>` both call it by.
 *
 * Named, NOT pathed: where the bytes land belongs to whoever holds them. The
 * packager puts them at OEBPS/images/NAME; the CLI writes them into the
 * picture folder beside the `.fountain` (src/library-images.ts). Defined
 * here, the lower layer, so convert.ts shares the one shape without the
 * packager importing the orchestrator that calls it.
 */
export interface BookImage {
  name: string;
  bytes: Uint8Array;
}

/**
 * Manifest ids for picture filenames, in order: `p001-01.png` becomes
 * `img-p001-01`.
 *
 * The prefix is load-bearing. An id is an XML name, which may not start with
 * a digit, and `9-plate.png` clears notes.ts's filename allowlist. The
 * character class is narrower than XML's own, which admits `.`; one
 * conservative set leaves nothing to a downstream matcher's idea of a name.
 *
 * That reduction is why uniqueness is not a property of the transform, and
 * why this takes the whole list and the ids already `taken`. `.` is inside
 * the allowlist and outside the class, so `a.b.png` and `a-b.png` both want
 * `img-a-b`. Our own writer cannot produce that pair (it names pictures
 * `pNNN-NN.png`), but a `.fountain` can be rendered against a folder somebody
 * else filled, and two items sharing an id is a hard failure, not a cosmetic
 * one: two ERROR(RSC-005)s plus an ERROR(RSC-008) for the picture whose item
 * the duplicate displaced (measured 2026-08-11). Each bump is checked against
 * everything assigned so far, so a folder that also holds `a-b-2.png` is
 * bumped again rather than colliding a second time.
 */
export function imageIds(names: readonly string[], taken: ReadonlySet<string>): string[] {
  const seen = new Set(taken);
  return names.map((name) => {
    const base = `img-${name.replace(/\.png$/, '').replace(/[^A-Za-z0-9_-]/g, '-')}`;
    let id = base;
    for (let n = 2; seen.has(id); n++) id = `${base}-${n}`;
    seen.add(id);
    return id;
  });
}

/**
 * The pictures to put in the container: each name the body shows, once, in
 * the order the body first shows it, with the first bytes passed under that
 * name.
 *
 * Only what the body shows, because a picture in the container that no page
 * points at is dead weight in a format where weight is the whole problem, and
 * epubcheck is clean about it, so nothing else would ever say so. Once,
 * because a second manifest item for one file is OPF-074.
 *
 * A name the body shows with no bytes behind it THROWS. The `<img>` is
 * already in the body, so the only alternative is a book that fails
 * validation (RSC-007, measured on images.pdf) and shows a broken picture,
 * with no channel back from here to say why: this returns bytes, not
 * warnings. It cannot happen to a caller that builds the renderer's `images`
 * set and this list from one array, since rendersImage shows only what that
 * set holds; a throw is how a caller that does not finds out, in its first
 * test rather than a reader's hands.
 */
function picturesToShip(used: ReadonlySet<string>, images: readonly BookImage[]): BookImage[] {
  const byName = new Map<string, BookImage>();
  for (const img of images) {
    if (used.has(img.name) && !byName.has(img.name)) byName.set(img.name, img);
  }
  const missing = [...used].filter((name) => !byName.has(name));
  if (missing.length > 0) {
    throw new Error(
      `The book shows ${missing.join(', ')} but was not given the picture, `
        + 'so it would point at a file it does not carry.',
    );
  }
  return [...used].map((name) => byName.get(name)!);
}

function containerXml(): string {
  return `<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
`;
}

function packageOpf(
  meta: BookMeta,
  body: BookBody,
  modified: string,
  includeTitlePage: boolean,
  images: readonly BookImage[],
): string {
  const manifest = body.files
    .map((f) => `    <item id="${f.id}" href="text/${f.id}.xhtml" media-type="application/xhtml+xml"/>`)
    .join('\n');
  // Pictures are RESOURCES: manifested, never in the spine (an itemref must
  // name a content document), and with no `properties`, which is for nav,
  // cover-image, scripted, svg and remote-resources, none of which a plate in
  // the flow is. The href is relative to this file at OEBPS/, so
  // `images/NAME`, while the body at OEBPS/text/ spells the same file
  // `../images/NAME`. `image/png` holds because only allowlisted `.png` names
  // reach here (usedImages is filled by rendersImage's yes alone), and every
  // caller's bytes are PNGs it made or checked.
  //
  // Ids are assigned against every id this document writes for itself, the
  // metadata's `uid` included, since XML ids are unique across the whole
  // document. The `img-` prefix already keeps clear of all of them; the set
  // keeps that true for an item added later with an `img-` id of its own.
  const ids = imageIds(
    images.map((img) => img.name),
    new Set(['uid', 'nav', 'css', 'titlepage', ...body.files.map((f) => f.id)]),
  );
  const imageItems = images
    .map((img, i) => `\n    <item id="${ids[i]}" href="images/${escapeXml(img.name)}" media-type="image/png"/>`)
    .join('');
  const spine = body.files.map((f) => `    <itemref idref="${f.id}"/>`).join('\n');
  const creator = meta.author ? `    <dc:creator>${escapeXml(meta.author)}</dc:creator>\n` : '';

  const titleItem = includeTitlePage
    ? '    <item id="titlepage" href="titlepage.xhtml" media-type="application/xhtml+xml"/>\n'
    : '';
  const titleRef = includeTitlePage ? '    <itemref idref="titlepage"/>\n' : '';
  // Apple Books otherwise ignores our publisher font-family and lets the
  // reader's own Justify setting override text-align — the ibooks:
  // specified-fonts meta below is what keeps Books honoring both. The
  // prefix declaration is what keeps epubcheck quiet about a
  // non-standard property; every other reading system just ignores it.
  return `<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" xml:lang="en" prefix="ibooks: http://vocabulary.itunes.apple.com/rdf/ibooks/vocabulary-extensions-1.0/">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">${escapeXml(meta.identifier!)}</dc:identifier>
    <dc:title>${escapeXml(meta.title)}</dc:title>
${creator}    <dc:language>${meta.language ?? 'en'}</dc:language>
    <meta property="dcterms:modified">${modified}</meta>
    <meta property="ibooks:specified-fonts">true</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="css" href="style.css" media-type="text/css"/>
${titleItem}${manifest}${imageItems}
  </manifest>
  <spine>
${titleRef}${spine}
  </spine>
</package>
`;
}

function navXhtml(meta: BookMeta, body: BookBody, includeTitlePage: boolean): string {
  const items = body.toc
    .map((e) => `      <li><a href="${e.href}">${escapeXml(e.title)}</a></li>`)
    .join('\n');

  // The script's ORIGINAL printed pagination, so capable readers can show
  // real page numbers and jump by page. Only meaningful when the book
  // actually carries markers (each href points at a pagebreak anchor).
  const pageListNav = body.pageList.length
    ? `<nav epub:type="page-list" hidden="hidden">
  <ol>
${body.pageList.map((p) => `    <li><a href="${p.href}">${escapeXml(p.label)}</a></li>`).join('\n')}
  </ol>
</nav>
`
    : '';

  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
<title>${escapeXml(meta.title)}</title>
<link rel="stylesheet" type="text/css" href="style.css"/>
</head>
<body>
<nav epub:type="toc" id="toc">
  <h1>Scenes</h1>
  <ol>
${items}
  </ol>
</nav>
<nav epub:type="landmarks" hidden="hidden">
  <ol>
${includeTitlePage ? '    <li><a epub:type="titlepage" href="titlepage.xhtml">Title Page</a></li>\n' : ''}    <li><a epub:type="bodymatter" href="text/${body.files[0]?.id ?? 'body001'}.xhtml">Begin Reading</a></li>
  </ol>
</nav>
${pageListNav}</body>
</html>
`;
}

function titlePageXhtml(meta: BookMeta): string {
  const author = meta.author
    ? `<p class="credit">Written by</p>\n<p class="author">${escapeXml(meta.author)}</p>\n`
    : '';
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
<title>${escapeXml(meta.title)}</title>
<link rel="stylesheet" type="text/css" href="style.css"/>
</head>
<body>
<section class="titlepage" epub:type="titlepage">
<h1>${escapeXml(meta.title)}</h1>
${author}</section>
</body>
</html>
`;
}

/**
 * Assemble a complete EPUB3 file.
 *
 * `images` is every picture the caller HAS; `body.usedImages` is what the
 * pages show, and the container carries exactly those (picturesToShip says
 * why both halves of "exactly" matter). Empty by default, which is right for
 * every body that shows no picture and a thrown error for one that does.
 */
export async function buildEpub(
  meta: BookMeta,
  body: BookBody,
  format: FormatOptions = DEFAULT_FORMAT_OPTIONS,
  images: readonly BookImage[] = [],
): Promise<Uint8Array> {
  const shipped = picturesToShip(body.usedImages, images);
  // ?? rather than spread-with-default: a caller passing an explicit
  // `identifier: undefined` would clobber a spread default and crash the
  // non-null assertion downstream.
  const resolved: BookMeta = {
    ...meta,
    identifier: meta.identifier ?? `urn:uuid:${crypto.randomUUID()}`,
  };
  const modified = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');

  const zip = new JSZip();
  // OCF: mimetype must be the first entry and stored uncompressed.
  zip.file('mimetype', 'application/epub+zip', { compression: 'STORE' });
  zip.file('META-INF/container.xml', containerXml());
  zip.file('OEBPS/package.opf', packageOpf(resolved, body, modified, format.includeTitlePage, shipped));
  zip.file('OEBPS/nav.xhtml', navXhtml(resolved, body, format.includeTitlePage));
  // The figure rules ride only in a book that shows a figure, so a book
  // without pictures carries the sheet it always did. Asked of the body, as
  // the preview asks it; picturesToShip has already made the two agree.
  zip.file('OEBPS/style.css', screenplayCss(format, { hasFigures: body.usedImages.size > 0 }));
  if (format.includeTitlePage) {
    zip.file('OEBPS/titlepage.xhtml', titlePageXhtml(resolved));
  }
  for (const f of body.files) {
    zip.file(`OEBPS/text/${f.id}.xhtml`, f.xhtml);
  }
  // The same list, in the same order, as the manifest items above, so the
  // container and the package document cannot disagree about what is in the
  // book. STORED: a PNG is already deflated, and deflating it again saved
  // 0.3% across sixty plates for about 330 ms (measured 2026-08-11), a cost
  // paid on every Tune move, since Tune rebuilds the book each time.
  for (const img of shipped) {
    zip.file(`OEBPS/images/${img.name}`, img.bytes, { compression: 'STORE' });
  }

  return zip.generateAsync({
    type: 'uint8array',
    compression: 'DEFLATE',
    compressionOptions: { level: 9 },
  });
}
