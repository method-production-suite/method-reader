// The KF8 markup: our XHTML turned into what a Kindle's KF8 reader expects,
// and cut into skeletons and fragments.
//
// Ported from Calibre 9.11.0's src/calibre/ebooks/mobi/writer8/main.py (the
// markup passes: resource links, CSS flows, link placeholders, aid
// attributes) and writer8/skeleton.py (the Chunker). Calibre is GPL-3,
// Copyright Kovid Goyal; this port is part of Screepub, AGPL-3.0-or-later
// (see THIRD-PARTY-NOTICES.md).
//
// How KF8 stores a document: each XHTML file becomes a SKELETON (the file
// with most of its body taken out) followed by FRAGMENTS (the pieces taken
// out, each at most 8192 bytes). The skeleton table says where each file's
// skeleton sits in the text; the fragment table says, for each piece, the
// position in the rebuilt file to put it back at. A reader rebuilds a file
// by inserting its fragments into its skeleton in order. Every element that
// can be a link target gets an `aid` attribute, and links and the TOC point
// at a fragment and an offset into it ("pos:fid") rather than at an id.
//
// What is left out, because our EPUB never carries it: SVG (inline or as
// files) and `@import` between stylesheets. `<style>` elements are handled,
// though src/epub/ writes none.

import { toBase, type ChunkEntry, type SkelEntry } from './index';
import type { EpubBook } from './epub';
import { resolveHref, splitFragment } from './epub';
import {
  descendants, escapeText, getAttr, localName, openTag, removeAttr, serialize, setAttr,
  type XElement, type XNode,
} from './xml';

export const CHUNK_SIZE = 8192;
const XHTML_NS = 'http://www.w3.org/1999/xhtml';

/** Tags kindlegen gives an aid (skeleton.py's aid_able_tags). */
export const AID_ABLE_TAGS: ReadonlySet<string> = new Set([
  'a', 'abbr', 'address', 'article', 'aside', 'audio', 'b', 'bdo', 'blockquote', 'body', 'button',
  'cite', 'code', 'dd', 'del', 'details', 'dfn', 'div', 'dl', 'dt', 'em', 'fieldset', 'figcaption',
  'figure', 'footer', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'header', 'hgroup', 'i', 'ins', 'kbd',
  'label', 'legend', 'li', 'map', 'mark', 'meter', 'nav', 'ol', 'output', 'p', 'pre', 'progress',
  'q', 'rp', 'rt', 'samp', 'section', 'select', 'small', 'span', 'strong', 'sub', 'summary', 'sup',
  'textarea', 'time', 'ul', 'var', 'video',
]);

/** Written `<p></p>`, never `<p/>` (skeleton.py's close_self_closing_tags). */
const CLOSE_EMPTY: ReadonlySet<string> = new Set([...AID_ABLE_TAGS, 'script', 'style', 'title', 'head']);

const enc = new TextEncoder();
const utf8 = (s: string) => enc.encode(s);
const ser = (n: XNode) => serialize(n, { closeEmpty: CLOSE_EMPTY });

/** A record reference: base 32, four digits (main.py's to_ref). */
export const toRef = (n: number) => toBase(n, 32, 4);
/** A link offset: base 32, ten digits (skeleton.py's to_href). */
const toHref = (n: number) => toBase(n, 32, 10);

export interface ResourceRef {
  /** 1-based resource number, as `kindle:embed:XXXX` counts it. */
  index: number;
  mime: string;
}

export interface Kf8Markup {
  /** Every flow, encoded: flow 0 is the text (each file's skeleton then its
   * fragments), the rest are stylesheets. */
  flows: Uint8Array[];
  skelTable: SkelEntry[];
  chunkTable: ChunkEntry[];
  /** aid → [fragment number, offset in it, offset in the rebuilt text]. */
  aidOffsets: Map<string, [number, number, number]>;
  /** `href` + NUL + `id` → aid; the empty id is the file's body. */
  idMap: Map<string, string>;
  /** Package hrefs of the pictures the markup shows. */
  usedImages: Set<string>;
}

const key = (href: string, id: string) => `${href}\u0000${id}`;

interface Chunk {
  raw: Uint8Array;
  startsTags: string[];
  endsTags: string[];
  insertPos: number;
  selector: string;
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a);
  out.set(b, a.length);
  return out;
}

function dirOf(href: string): string {
  const slash = href.lastIndexOf('/');
  return slash < 0 ? '' : href.slice(0, slash);
}

function* elementsOf(root: XElement): Generator<XElement> {
  yield root;
  yield* descendants(root);
}

function textOf(nodes: XNode[]): string {
  return nodes.map((n) => (n.kind === 'text' ? n.text : '')).join('');
}

function mergeText(el: XElement): void {
  const out: XNode[] = [];
  for (const c of el.children) {
    const last = out[out.length - 1];
    if (c.kind === 'text' && last?.kind === 'text') last.text += c.text;
    else if (c.kind !== 'text' || c.text !== '') out.push(c);
  }
  el.children = out;
  for (const c of el.children) if (c.kind === 'element') mergeText(c);
}

/** A copy with every namespace taken off names and attributes, and the
 * namespace declarations gone, so it serializes as plain HTML 5 markup; the
 * XHTML namespace goes back on <html> when the skeleton is written
 * (skeleton.py's remove_namespaces). `epub:type` becomes `type`, as there. */
function removeNamespaces(root: XElement): XElement {
  const strip = (el: XElement): XElement => {
    const attrs = new Map<string, string>();
    for (const [k, v] of el.attrs) {
      if (k === 'xmlns' || k.startsWith('xmlns:')) continue;
      attrs.set(localName(k), v);
    }
    return {
      kind: 'element',
      name: localName(el.name),
      attrs: [...attrs],
      children: el.children.map((c) => (c.kind === 'text' ? { ...c } : strip(c))),
    };
  };
  const copy = strip(root);
  // The root keeps only its language, class and style.
  const lang = root.attrs.filter(([k]) => localName(k) === 'lang').at(-1)?.[1];
  copy.attrs = [
    ...(lang ? [['lang', lang] as [string, string]] : []),
    ...copy.attrs.filter(([k]) => k === 'class' || k === 'style'),
  ];
  copy.name = 'html';
  return copy;
}

class Chunker {
  private selector = '';
  readonly chunks: Chunk[] = [];

  private chunkUpText(text: string): Chunk[] {
    let raw = utf8(escapeText(text));
    const out: Uint8Array[] = [];
    const take = () => {
      if (raw.length <= CHUNK_SIZE) {
        const all = raw;
        raw = new Uint8Array(0);
        return all;
      }
      let end = CHUNK_SIZE;
      while (end > 0 && (raw[end] & 0xc0) === 0x80) end--;
      // Nor inside an entity: the text is escaped before it is cut, and an
      // "&amp;" split into "&a" and "mp;" leaves both pieces broken XML.
      // Calibre's chunk_up_text cuts at the byte count alone; this is the
      // one place the port does not follow it. Our escapes are at most five
      // bytes, so an "&" in the last four with no ";" after it is one cut.
      for (let k = end - 1; k >= Math.max(0, end - 4); k--) {
        if (raw[k] === 0x3b) break; // ';'
        if (raw[k] === 0x26) { // '&'
          end = k;
          break;
        }
      }
      const piece = raw.slice(0, end);
      raw = raw.slice(end);
      return piece;
    };
    out.push(take());
    while (raw.length > 0) {
      out.push(concat(concat(utf8('<span class="AmznBigTextBlock">'), take()), utf8('</span>')));
    }
    return out.map((r) => ({ raw: r, startsTags: [], endsTags: [], insertPos: 0, selector: this.selector }));
  }

  /** skeleton.py's step_into_tag: take `tag`'s content out into fragments,
   * stepping into any child too large for one fragment. */
  stepIntoTag(tag: XElement): void {
    const aid = getAttr(tag, 'aid')!;
    this.selector = `P-//*[@aid='${aid}']`;
    const chunks = this.chunks;
    const first = chunks.length;

    // Leading text; pure whitespace stays in the skeleton.
    let k = 0;
    const lead = tag.children[0];
    if (lead?.kind === 'text') {
      if (lead.text.trim()) {
        chunks.push(...this.chunkUpText(lead.text));
        tag.children.splice(0, 1);
      } else {
        k = 1;
      }
    }

    while (k < tag.children.length) {
      const child = tag.children[k];
      if (child.kind === 'text') {
        k++;
        continue;
      }
      const next = tag.children[k + 1];
      const tail = next?.kind === 'text' ? next : undefined;
      const raw = utf8(ser(child));
      if (raw.length > CHUNK_SIZE && getAttr(child, 'aid') !== undefined) {
        this.stepIntoTag(child);
        if (tail && tail.text.trim()) {
          chunks.push(...this.chunkUpText(tail.text));
          tag.children.splice(k + 1, 1);
          k += 1;
        } else {
          k += tail ? 2 : 1;
        }
      } else {
        chunks.push({ raw, startsTags: [], endsTags: [], insertPos: 0, selector: this.selector });
        if (tail) chunks.push(...this.chunkUpText(tail.text));
        tag.children.splice(k, tail ? 2 : 1);
      }
    }

    if (chunks.length <= first && chunks.length > 0) {
      throw new Error('Stepped into an element that made no fragments.');
    }
    if (chunks.length > 0) {
      chunks[first].startsTags.push(aid);
      chunks[chunks.length - 1].endsTags.push(aid);
    }
    this.selector = `S-//*[@aid='${aid}']`;
  }
}

/** Neighbouring small fragments of one element merge, up to the size limit
 * (skeleton.py's merge_small_chunks). */
function mergeSmallChunks(chunks: Chunk[]): Chunk[] {
  const out = chunks.slice(0, 1);
  for (const chunk of chunks.slice(1)) {
    const prev = out[out.length - 1];
    if (chunk.startsTags.length > 0 || chunk.raw.length + prev.raw.length > CHUNK_SIZE || prev.endsTags.length > 0) {
      out.push(chunk);
    } else {
      prev.raw = concat(prev.raw, chunk.raw);
      prev.endsTags = chunk.endsTags;
    }
  }
  return out;
}

interface Skeleton {
  skeleton: Uint8Array;
  chunks: Chunk[];
  startPos: number;
}

/** skeleton.py's Skeleton: render what is left of the file, measure each
 * remaining aid element, and work out where each fragment goes back. */
function makeSkeleton(root: XElement, chunks: Chunk[]): Skeleton {
  const rendered = `<?xml version="1.0" encoding="UTF-8"?>\n${ser(root).replace('<html', `<html xmlns="${XHTML_NS}"`)}\n`;
  const skeleton = utf8(rendered);
  const bodyOffset = utf8(rendered.slice(0, rendered.indexOf('<body'))).length;

  // For each aid element still in the skeleton: the bytes from its start to
  // where its content begins, and from where its content ends to the end of
  // its tail.
  const metrics = new Map<string, { start: number; end: number }>();
  const walk = (el: XElement) => {
    el.children.forEach((c, i) => {
      if (c.kind !== 'element') return;
      const aid = getAttr(c, 'aid');
      if (aid !== undefined) {
        const lead = c.children[0]?.kind === 'text' ? utf8(escapeText(textOf([c.children[0]]))).length : 0;
        const after = el.children[i + 1];
        const tail = after?.kind === 'text' ? utf8(escapeText(after.text)).length : 0;
        metrics.set(aid, { start: utf8(openTag(c)).length + 1 + lead, end: `</${c.name}>`.length + tail });
      }
      walk(c);
    });
  };
  walk(root);

  let pos = bodyOffset;
  for (const chunk of chunks) {
    for (const t of chunk.startsTags) pos += metrics.get(t)!.start;
    chunk.insertPos = pos;
    pos += chunk.raw.length;
    for (const t of chunk.endsTags) pos += metrics.get(t)!.end;
  }
  return { skeleton, chunks, startPos: 0 };
}

function rebuild(s: Skeleton): Uint8Array {
  let out = s.skeleton;
  for (const c of s.chunks) {
    const next = new Uint8Array(out.length + c.raw.length);
    next.set(out.subarray(0, c.insertPos));
    next.set(c.raw, c.insertPos);
    next.set(out.subarray(c.insertPos), c.insertPos + c.raw.length);
    out = next;
  }
  return out;
}

const skelLength = (s: Skeleton) => s.skeleton.length + s.chunks.reduce((n, c) => n + c.raw.length, 0);

/**
 * Turn the EPUB's spine into KF8 text and tables. `resources` maps a
 * picture's package href to its resource number and mime type. The book's
 * documents are changed in place.
 */
export function kf8Markup(book: EpubBook, resources: ReadonlyMap<string, ResourceRef>): Kf8Markup {
  const usedImages = new Set<string>();
  const flows: (Uint8Array | string)[] = [new Uint8Array(0)];
  const spineHrefs = new Set(book.spine.map((s) => s.href));

  // main.py's cleanup_markup: empty scripts go; any aid or cid already in
  // the markup goes, since aids are this writer's to assign.
  for (const { doc } of book.spine) {
    for (const el of [...elementsOf(doc.root)]) {
      el.children = el.children.filter((c) => !(c.kind === 'element' && localName(c.name) === 'script'
        && c.children.length === 0 && getAttr(c, 'src') === undefined));
      removeAttr(el, 'aid');
      removeAttr(el, 'cid');
    }
  }

  // replace_resource_links: pictures become kindle:embed references.
  for (const { href, doc } of book.spine) {
    for (const el of elementsOf(doc.root)) {
      const name = localName(el.name);
      if (name !== 'img' && name !== 'image') continue;
      for (const pair of el.attrs) {
        const attr = localName(pair[0]).toLowerCase();
        if (attr !== 'src' && attr !== 'href') continue;
        const target = splitFragment(resolveHref(dirOf(href), pair[1]))[0];
        const ref = resources.get(target);
        if (ref) {
          usedImages.add(target);
          pair[1] = `kindle:embed:${toRef(ref.index)}?mime=${ref.mime}`;
        }
      }
    }
  }

  // extract_css_into_flows: each stylesheet is a flow, linked as
  // kindle:flow; an inline <style> becomes a flow of its own, once per text.
  const sheets = new Map<string, number>();
  for (const [href, css] of book.stylesheets) {
    sheets.set(href, flows.length);
    flows.push(css);
  }
  const inline = new Map<string, number>();
  for (const { href, doc } of book.spine) {
    for (const el of [...elementsOf(doc.root)]) {
      if (localName(el.name) === 'link' && getAttr(el, 'href') !== undefined) {
        const idx = sheets.get(resolveHref(dirOf(href), getAttr(el, 'href')!));
        if (idx !== undefined) setAttr(el, 'href', `kindle:flow:${toRef(idx)}?mime=text/css`);
      }
      el.children = el.children.flatMap((c): XNode[] => {
        if (c.kind !== 'element' || localName(c.name) !== 'style') return [c];
        const raw = textOf(c.children);
        if (!raw.trim()) return [];
        let idx = inline.get(raw);
        if (idx === undefined) {
          idx = flows.length;
          flows.push(raw);
          inline.set(raw, idx);
        }
        const link: XElement = {
          kind: 'element',
          name: 'link',
          attrs: [['type', 'text/css'], ['rel', 'stylesheet'], ['href', `kindle:flow:${toRef(idx)}?mime=text/css`]],
          children: [],
        };
        return [link, { kind: 'text', text: '\n' }];
      });
    }
    mergeText(doc.root);
  }

  // replace_internal_links_with_placeholders: a link to a spine document
  // gets a placeholder, filled in once the fragments are known.
  const linkMap = new Map<string, [string, string]>();
  let count = 0;
  for (const { href, doc } of book.spine) {
    for (const el of elementsOf(doc.root)) {
      if (localName(el.name) !== 'a' || getAttr(el, 'href') === undefined) continue;
      count++;
      const [target, frag] = splitFragment(resolveHref(dirOf(href), getAttr(el, 'href')!));
      if (spineHrefs.has(target)) {
        const placeholder = `kindle:pos:fid:0000:off:${toHref(count)}`;
        linkMap.set(placeholder, [target, frag ?? '']);
        setAttr(el, 'href', placeholder);
      }
    }
  }

  // insert_aid_attributes. Inside a table only an element with an id gets
  // one, a cid ("The Kindle renderer barfs on large tables that have aid on
  // any of their tags", Calibre bug 1489495).
  const idMap = new Map<string, string>();
  let cid = 0;
  book.spine.forEach(({ href, doc }, i) => {
    const aidBase = i * 1_000_000;
    let j = 0;
    const visit = (el: XElement, inTable: boolean) => {
      for (const c of el.children) {
        if (c.kind !== 'element') continue;
        let id = getAttr(c, 'id');
        const tagName = localName(c.name).toLowerCase();
        if (id === undefined && tagName === 'a') {
          id = getAttr(c, 'name');
          if (id !== undefined) setAttr(c, 'id', id);
        }
        const table = tagName === 'table' || inTable;
        if (id !== undefined || AID_ABLE_TAGS.has(tagName)) {
          if (table) {
            if (id) {
              cid++;
              const val = `c${cid}`;
              idMap.set(key(href, id), val);
              setAttr(c, 'cid', val);
            }
          } else {
            const aid = toBase(aidBase + j, 32);
            setAttr(c, 'aid', aid);
            if (tagName === 'body') idMap.set(key(href, ''), aid);
            if (id !== undefined) idMap.set(key(href, id), aid);
            j++;
          }
        }
        visit(c, table);
      }
    };
    visit(doc.root, false);
  });

  // The Chunker.
  const skeletons: Skeleton[] = book.spine.map(({ doc }) => {
    const root = removeNamespaces(doc.root);
    // kindlegen always puts the aid last.
    for (const el of elementsOf(root)) {
      const aid = getAttr(el, 'aid');
      if (aid !== undefined) {
        removeAttr(el, 'aid');
        el.attrs.push(['aid', aid]);
      }
    }
    const body = [...elementsOf(root)].find((e) => e.name === 'body');
    if (!body) throw new Error('A spine document has no <body>.');
    // body.tail = '\n'
    const parent = [...elementsOf(root)].find((e) => e.children.includes(body))!;
    const at = parent.children.indexOf(body);
    const after = parent.children[at + 1];
    if (after?.kind === 'text') after.text = '\n';
    else parent.children.splice(at + 1, 0, { kind: 'text', text: '\n' });

    const chunker = new Chunker();
    chunker.stepIntoTag(body);
    return makeSkeleton(root, mergeSmallChunks(chunker.chunks));
  });

  // create_tables.
  const skelTable: SkelEntry[] = [];
  const chunkTable: ChunkEntry[] = [];
  let sp = 0;
  skeletons.forEach((s, fileNumber) => {
    s.startPos = sp;
    sp += skelLength(s);
    skelTable.push({ fileNumber, chunkCount: s.chunks.length, startPos: s.startPos, length: s.skeleton.length });
  });
  let seq = 0;
  skeletons.forEach((s, fileNumber) => {
    let cp = 0;
    for (const c of s.chunks) {
      chunkTable.push({
        insertPos: c.insertPos + s.startPos,
        selector: c.selector,
        fileNumber,
        sequenceNumber: seq++,
        startPos: cp,
        length: c.raw.length,
      });
      cp += c.raw.length;
    }
  });

  // set_internal_links: where each aid lands, from the REBUILT text, as a
  // fragment number and an offset into it.
  const stored = joinBytes(skeletons.flatMap((s) => [s.skeleton, ...s.chunks.map((c) => c.raw)]));
  const rebuilt = new TextDecoder().decode(joinBytes(skeletons.map(rebuild)));
  const aidOffsets = new Map<string, [number, number, number]>();
  const AID_RE = /<[^>]+? [ac]id=['"]([cA-Z0-9]+)['"]/g;
  let byteAt = 0;
  let charAt = 0;
  let c = 0;
  for (const m of rebuilt.matchAll(AID_RE)) {
    // Character index to byte offset, incrementally.
    byteAt += utf8(rebuilt.slice(charAt, m.index)).length;
    charAt = m.index!;
    const offset = byteAt;
    while (c < chunkTable.length - 1 && chunkTable[c].insertPos + chunkTable[c].length <= offset) c++;
    const chunk = chunkTable[c];
    if (!chunk) throw new Error(`No fragment holds aid ${m[1]}.`);
    let posFid: [number, number, number];
    if (chunk.insertPos <= offset) posFid = [chunk.sequenceNumber, offset - chunk.insertPos, offset];
    else posFid = [chunk.sequenceNumber, 0, offset]; // in the skeleton: the fragment after it
    aidOffsets.set(m[1], posFid);
  }

  // Fill the link placeholders in the stored text; same length, so nothing
  // moves.
  let text = stored;
  if (linkMap.size > 0) {
    let s = new TextDecoder().decode(stored);
    s = s.replace(/<[^>]+(kindle:pos:fid:0000:off:[0-9A-Za-z]{10})/g, (raw, placeholder: string) => {
      const target = linkMap.get(placeholder);
      if (!target) return raw;
      const aid = idMap.get(key(target[0], target[1])) ?? idMap.get(key(target[0], ''));
      const at = aid === undefined ? undefined : aidOffsets.get(aid);
      if (!at) return raw;
      return `${raw.slice(0, -19)}${toRef(at[0])}:off:${toHref(at[1])}`;
    });
    text = utf8(s);
  }

  flows[0] = text;
  return {
    flows: flows.map((f) => (typeof f === 'string' ? utf8(f) : f)),
    skelTable,
    chunkTable,
    aidOffsets,
    idMap,
    usedImages,
  };
}

function joinBytes(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/** idMap's key, for callers looking up a TOC or guide target. */
export function idKey(href: string, id: string): string {
  return key(href, id);
}
