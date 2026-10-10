// Calibre's own reader, run on an AZW3, for the AZW3 tests. ONLY under bun
// test: tests/isolate-calibre-config.ts has pointed CALIBRE_CONFIG_DIRECTORY
// at a scratch copy by the time any test runs, and every spawn here passes
// process.env so Calibre sees that copy and never the real folder. The cwd
// is the book's own scratch folder, because `calibre-debug --inspect-mobi`
// writes its decompiled folder into the working directory.
import { existsSync, readFileSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import JSZip from 'jszip';
import { calibreTool } from '../../src/export/calibre';
import { readEpub } from '../../src/azw3/epub';
import { findAll, getAttr, parseXml, textContent } from '../../src/azw3/xml';

export const ebookConvert = calibreTool('ebook-convert');
export const calibreDebug = calibreTool('calibre-debug');

export function runCalibreTool(tool: string, args: string[], cwd: string): { code: number; out: string } {
  const p = Bun.spawnSync([tool, ...args], { cwd, env: process.env, stdout: 'pipe', stderr: 'pipe' });
  return { code: p.exitCode ?? -1, out: `${p.stdout.toString()}\n${p.stderr.toString()}` };
}

const norm = (s: string) => s.replace(/[\s­]+/g, ' ').trim();

/** What a reader of an EPUB sees: title, authors, language, the TOC's
 * labels, the text of every block in order, and the pictures. */
export interface BookView {
  title: string;
  authors: string[];
  language: string;
  toc: string[];
  /** The text of each p, h1 and h2, in reading order. */
  blocks: string[];
  /** All the body text, whitespace folded. */
  text: string;
  images: Uint8Array[];
  cover: Uint8Array | null;
}

/** Our own EPUB, as a BookView. */
export async function viewOfOurEpub(bytes: Uint8Array): Promise<BookView> {
  const book = await readEpub(bytes);
  const bodies = book.spine.map((s) => findAll(s.doc.root, 'body')[0]);
  return {
    title: book.title,
    authors: book.authors,
    language: book.language,
    toc: book.toc.map((t) => t.title),
    blocks: bodies.flatMap((b) => blocksOf(b)),
    text: norm(bodies.map((b) => textContent(b)).join(' ')),
    images: book.images.map((i) => i.bytes),
    cover: book.images.find((i) => i.href === book.coverHref)?.bytes ?? null,
  };
}

function blocksOf(body: ReturnType<typeof parseXml>['root']): string[] {
  const out: string[] = [];
  const walk = (el: typeof body) => {
    for (const c of el.children) {
      if (c.kind !== 'element') continue;
      const name = c.name.replace(/^.*:/, '');
      if (name === 'p' || name === 'h1' || name === 'h2') {
        const t = norm(textContent(c));
        if (t) out.push(t);
      } else {
        walk(c);
      }
    }
  };
  walk(body);
  return out;
}

/** An EPUB Calibre wrote (OPF 2, an NCX), as a BookView. Calibre's
 * generated cover page, the one marked calibre:cover, is left out of the
 * text: it is Calibre's, not the book's. */
export async function viewOfCalibreEpub(bytes: Uint8Array): Promise<BookView> {
  const zip = await JSZip.loadAsync(bytes);
  const container = parseXml(await zip.file('META-INF/container.xml')!.async('string')).root;
  const opfPath = getAttr(findAll(container, 'rootfile')[0], 'full-path')!;
  const opfDir = opfPath.includes('/') ? opfPath.slice(0, opfPath.lastIndexOf('/') + 1) : '';
  const opf = parseXml(await zip.file(opfPath)!.async('string')).root;
  const items = new Map(findAll(opf, 'item').map((i) => [getAttr(i, 'id')!, i]));
  const meta = findAll(opf, 'metadata')[0];
  const dc = (n: string) => findAll(meta, n).map((e) => textContent(e).trim());

  const bodies = [];
  for (const ref of findAll(opf, 'itemref')) {
    const item = items.get(getAttr(ref, 'idref')!)!;
    const doc = parseXml(await zip.file(opfDir + getAttr(item, 'href')!)!.async('string')).root;
    if (findAll(doc, 'meta').some((m) => getAttr(m, 'name') === 'calibre:cover')) continue;
    bodies.push(findAll(doc, 'body')[0]);
  }

  const ncxItem = [...items.values()].find((i) => getAttr(i, 'media-type') === 'application/x-dtbncx+xml');
  const toc = ncxItem
    ? findAll(parseXml(await zip.file(opfDir + getAttr(ncxItem, 'href')!)!.async('string')).root, 'navLabel').map((l) => norm(textContent(l)))
    : [];

  const coverId = findAll(meta, 'meta').find((m) => getAttr(m, 'name') === 'cover')?.attrs.find(([k]) => k === 'content')?.[1];
  const imageItems = [...items.values()].filter((i) => (getAttr(i, 'media-type') ?? '').startsWith('image/'));
  const bytesOf = (i: typeof imageItems[number]) => zip.file(opfDir + getAttr(i, 'href')!)!.async('uint8array');
  return {
    title: dc('title')[0],
    authors: dc('creator'),
    language: dc('language')[0],
    toc,
    blocks: bodies.flatMap((b) => blocksOf(b)),
    text: norm(bodies.map((b) => textContent(b)).join(' ')),
    images: await Promise.all(imageItems.map(bytesOf)),
    cover: coverId && items.get(coverId) ? await bytesOf(items.get(coverId)!) : null,
  };
}

/** Calibre's reader turns `azw3` back into an EPUB beside it. Without a
 * default cover: Calibre would otherwise draw one for a book that has none,
 * and that picture is Calibre's, not the book's. */
export function calibreToEpub(azw3: string): { epub: Uint8Array | null; out: string } {
  const out = join(dirname(azw3), `${basename(azw3, '.azw3')}.calibre-read.epub`);
  const run = runCalibreTool(ebookConvert!, [azw3, out, '--no-default-epub-cover'], dirname(azw3));
  return { epub: run.code === 0 && existsSync(out) ? new Uint8Array(readFileSync(out)) : null, out: run.out };
}

/** `calibre-debug --inspect-mobi`: the decompiled folder, and the entry
 * counts its index dumps report. */
export function inspectMobi(azw3: string): { code: number; out: string; dir: string; entries: Record<string, number> } {
  const run = runCalibreTool(calibreDebug!, ['--inspect-mobi', azw3], dirname(azw3));
  const dir = join(dirname(azw3), `decompiled_${basename(azw3, '.azw3')}`);
  const entries: Record<string, number> = {};
  for (const name of ['skel', 'chunks', 'ncx', 'guide']) {
    const file = join(dir, `${name}.record`);
    if (!existsSync(file)) continue;
    const m = /Total number of actual Index Entries in all records: (\d+)/.exec(readFileSync(file, 'utf8'));
    if (m) entries[name] = Number(m[1]);
  }
  return { ...run, dir, entries };
}
