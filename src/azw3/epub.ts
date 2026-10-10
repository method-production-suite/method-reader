// The EPUB, read back out of its zip for the AZW3 writer: the same bytes
// every other route sends, so the Kindle file can never disagree with the
// book it was made from.
//
// Written for the EPUBs src/epub/build.ts makes (EPUB 3, one package, a nav
// document, PNG pictures), and strict about them: a missing package, spine
// item or picture throws rather than leaving a hole in the Kindle file.

import JSZip from 'jszip';
import { childElements, findAll, getAttr, localName, parseXml, textContent, type XDocument, type XElement } from './xml';

export interface EpubTocEntry {
  title: string;
  /** Package-relative path, with any `#fragment`. */
  href: string;
  /** 0 at the top level. */
  depth: number;
  children: EpubTocEntry[];
}

export interface EpubLandmark {
  /** The nav's `epub:type`: `bodymatter`, `titlepage`, `toc`... */
  type: string;
  title: string;
  href: string;
}

export interface EpubImage {
  href: string;
  mediaType: string;
  bytes: Uint8Array;
}

export interface EpubBook {
  title: string;
  authors: string[];
  language: string;
  identifier?: string;
  /** dcterms:modified, when present. */
  modified?: string;
  /** Content documents in reading order. */
  spine: { href: string; doc: XDocument }[];
  /** Stylesheets in manifest order: href → text. */
  stylesheets: Map<string, string>;
  /** Raster pictures in manifest order. */
  images: EpubImage[];
  /** The cover picture's href, when the package names one. */
  coverHref?: string;
  toc: EpubTocEntry[];
  landmarks: EpubLandmark[];
}

const RASTER = new Set(['image/png', 'image/jpeg', 'image/gif']);

/** Resolve `rel` against the folder `base` (both zip paths), with `..` and
 * `.` folded, percent-escapes decoded and any fragment kept. */
export function resolveHref(base: string, rel: string): string {
  const [path, fragment] = splitFragment(rel);
  const parts = path.startsWith('/') ? [] : base.split('/').filter(Boolean);
  for (const seg of decodeURIComponent(path).split('/')) {
    if (seg === '' || seg === '.') continue;
    if (seg === '..') parts.pop();
    else parts.push(seg);
  }
  const joined = parts.join('/');
  return fragment === undefined ? joined : `${joined}#${fragment}`;
}

export function splitFragment(href: string): [string, string | undefined] {
  const hash = href.indexOf('#');
  return hash < 0 ? [href, undefined] : [href.slice(0, hash), href.slice(hash + 1)];
}

function dirOf(path: string): string {
  const slash = path.lastIndexOf('/');
  return slash < 0 ? '' : path.slice(0, slash);
}

/** Package-relative path for a zip path, given the package's folder. */
function relativeTo(opfDir: string, zipPath: string): string {
  if (!opfDir) return zipPath;
  if (zipPath === opfDir) return '';
  return zipPath.startsWith(`${opfDir}/`) ? zipPath.slice(opfDir.length + 1) : zipPath;
}

async function readText(zip: JSZip, path: string): Promise<string> {
  const file = zip.file(path);
  if (!file) throw new Error(`The EPUB has no ${path}.`);
  return file.async('string');
}

function navList(ol: XElement, base: string, depth: number): EpubTocEntry[] {
  const out: EpubTocEntry[] = [];
  for (const li of childElements(ol).filter((e) => localName(e.name) === 'li')) {
    const a = childElements(li).find((e) => localName(e.name) === 'a');
    const sub = childElements(li).find((e) => localName(e.name) === 'ol');
    const children = sub ? navList(sub, base, depth + 1) : [];
    const href = a ? getAttr(a, 'href') : undefined;
    if (a && href) {
      out.push({ title: textContent(a).trim(), href: resolveHref(base, href), depth, children });
    } else {
      // A heading with no link of its own: its children stand in its place.
      out.push(...children);
    }
  }
  return out;
}

function epubType(el: XElement): string | undefined {
  return el.attrs.find(([k]) => localName(k) === 'type' && k.includes(':'))?.[1];
}

export async function readEpub(bytes: Uint8Array): Promise<EpubBook> {
  const zip = await JSZip.loadAsync(bytes);
  const container = parseXml(await readText(zip, 'META-INF/container.xml'));
  const rootfile = findAll(container.root, 'rootfile')[0];
  const opfPath = rootfile && getAttr(rootfile, 'full-path');
  if (!opfPath) throw new Error('The EPUB names no package document.');
  const opfDir = dirOf(opfPath);
  const opf = parseXml(await readText(zip, opfPath)).root;

  const meta = findAll(opf, 'metadata')[0];
  const dc = (name: string) => (meta ? findAll(meta, name).map((e) => textContent(e).trim()).filter(Boolean) : []);
  const metas = meta ? findAll(meta, 'meta') : [];

  // Manifest: id → item, zip paths resolved from the package's folder.
  const items = findAll(opf, 'item').map((e) => ({
    id: getAttr(e, 'id') ?? '',
    zipPath: resolveHref(opfDir, getAttr(e, 'href') ?? ''),
    mediaType: getAttr(e, 'media-type') ?? '',
    properties: (getAttr(e, 'properties') ?? '').split(/\s+/).filter(Boolean),
  }));
  const byId = new Map(items.map((i) => [i.id, i]));

  const spine: EpubBook['spine'] = [];
  for (const ref of findAll(opf, 'itemref')) {
    const item = byId.get(getAttr(ref, 'idref') ?? '');
    if (!item) throw new Error(`The spine names ${getAttr(ref, 'idref')}, which the manifest does not hold.`);
    spine.push({ href: relativeTo(opfDir, item.zipPath), doc: parseXml(await readText(zip, item.zipPath)) });
  }

  const stylesheets = new Map<string, string>();
  const images: EpubImage[] = [];
  for (const item of items) {
    if (item.mediaType === 'text/css') {
      stylesheets.set(relativeTo(opfDir, item.zipPath), await readText(zip, item.zipPath));
    } else if (RASTER.has(item.mediaType)) {
      const file = zip.file(item.zipPath);
      if (!file) throw new Error(`The EPUB lists ${item.zipPath} but does not carry it.`);
      images.push({ href: relativeTo(opfDir, item.zipPath), mediaType: item.mediaType, bytes: await file.async('uint8array') });
    }
  }

  // The cover: EPUB 3's cover-image property, else OPF 2's meta.
  const coverMeta = metas.find((m) => getAttr(m, 'name') === 'cover');
  const coverItem = items.find((i) => i.properties.includes('cover-image'))
    ?? (coverMeta ? byId.get(getAttr(coverMeta, 'content') ?? '') : undefined);
  const coverHref = coverItem && RASTER.has(coverItem.mediaType) ? relativeTo(opfDir, coverItem.zipPath) : undefined;

  // The nav document: the table of contents and the landmarks.
  let toc: EpubTocEntry[] = [];
  const landmarks: EpubLandmark[] = [];
  const navItem = items.find((i) => i.properties.includes('nav'));
  if (navItem) {
    const nav = parseXml(await readText(zip, navItem.zipPath)).root;
    const navBase = relativeTo(opfDir, dirOf(navItem.zipPath));
    for (const n of findAll(nav, 'nav')) {
      const ol = childElements(n).find((e) => localName(e.name) === 'ol');
      if (!ol) continue;
      const type = epubType(n);
      if (type === 'toc') toc = navList(ol, navBase, 0);
      if (type === 'landmarks') {
        for (const a of findAll(ol, 'a')) {
          const href = getAttr(a, 'href');
          const t = epubType(a);
          if (href && t) landmarks.push({ type: t, title: textContent(a).trim(), href: resolveHref(navBase, href) });
        }
      }
    }
  }

  const modified = metas.find((m) => getAttr(m, 'property') === 'dcterms:modified');
  return {
    title: dc('title')[0] ?? 'Unknown',
    authors: dc('creator'),
    language: dc('language')[0] ?? 'en',
    identifier: dc('identifier')[0],
    modified: modified ? textContent(modified).trim() : undefined,
    spine,
    stylesheets,
    images,
    coverHref,
    toc,
    landmarks,
  };
}
