// A small XML reader and writer for the AZW3 writer: enough to take apart
// the XHTML, package and nav documents our own EPUB carries, change them,
// and write them back.
//
// The KF8 chunker (azw3/chunk.ts) depends on one property above all: a
// subtree serializes to exactly the bytes it occupies inside the serialized
// whole. Calibre gets that from lxml and has to patch lxml where it is not
// true (skeleton.py's tostring); here there is one serializer, written to be
// compositional, so it holds by construction.
//
// It reads well-formed XML only. Our EPUB is written by src/epub/ and
// checked by epubcheck, so a document this cannot read is a bug to hear
// about, not a page to guess at: it throws, naming what it met.

export interface XText {
  kind: 'text';
  text: string;
}

export interface XElement {
  kind: 'element';
  /** The qualified name as written: `p`, `dc:title`. */
  name: string;
  /** Attributes in document order, qualified names as written. */
  attrs: [string, string][];
  children: XNode[];
}

export type XNode = XElement | XText;

export interface XDocument {
  root: XElement;
}

const NAMED: Record<string, string> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };

function decodeEntities(raw: string): string {
  if (!raw.includes('&')) return raw;
  return raw.replace(/&([^;\s&]*);/g, (_whole, ref: string) => {
    if (ref in NAMED) return NAMED[ref];
    const num = /^#(?:x([0-9a-fA-F]+)|([0-9]+))$/.exec(ref);
    if (!num) throw new Error(`The document uses the entity &${ref};, which XML does not define.`);
    return String.fromCodePoint(num[1] !== undefined ? parseInt(num[1], 16) : parseInt(num[2], 10));
  });
}

const NAME_RE = /[A-Za-z_][\w.:-]*/y;
const SPACE_RE = /\s*/y;

/** Parse a whole document. Comments, processing instructions and the
 * doctype are dropped (kindlegen drops them too, skeleton.py notes); CDATA
 * becomes text; adjacent text is merged. */
export function parseXml(xml: string): XDocument {
  let i = 0;
  const stack: XElement[] = [];
  let root: XElement | null = null;

  const fail = (what: string): never => {
    throw new Error(`Could not read the XML: ${what} at offset ${i}.`);
  };
  const addText = (text: string) => {
    if (text === '') return;
    const parent = stack[stack.length - 1];
    if (!parent) {
      if (text.trim() !== '') fail('text outside the root element');
      return;
    }
    const last = parent.children[parent.children.length - 1];
    if (last && last.kind === 'text') last.text += text;
    else parent.children.push({ kind: 'text', text });
  };
  const readName = (): string => {
    NAME_RE.lastIndex = i;
    const m = NAME_RE.exec(xml);
    if (!m) fail('a missing name');
    i += m![0].length;
    return m![0];
  };
  const skipSpace = () => {
    SPACE_RE.lastIndex = i;
    i += SPACE_RE.exec(xml)![0].length;
  };

  while (i < xml.length) {
    const lt = xml.indexOf('<', i);
    if (lt === -1) {
      addText(decodeEntities(xml.slice(i)));
      break;
    }
    if (lt > i) addText(decodeEntities(xml.slice(i, lt)));
    i = lt;
    if (xml.startsWith('<!--', i)) {
      const end = xml.indexOf('-->', i + 4);
      if (end < 0) fail('an unclosed comment');
      i = end + 3;
    } else if (xml.startsWith('<![CDATA[', i)) {
      const end = xml.indexOf(']]>', i + 9);
      if (end < 0) fail('an unclosed CDATA section');
      addText(xml.slice(i + 9, end));
      i = end + 3;
    } else if (xml.startsWith('<?', i)) {
      const end = xml.indexOf('?>', i + 2);
      if (end < 0) fail('an unclosed processing instruction');
      i = end + 2;
    } else if (xml.startsWith('<!', i)) {
      // A doctype. Ours have no internal subset; one with brackets is
      // skipped to its closing bracket first.
      let end = xml.indexOf('>', i);
      const bracket = xml.indexOf('[', i);
      if (bracket >= 0 && bracket < end) end = xml.indexOf('>', xml.indexOf(']', bracket));
      if (end < 0) fail('an unclosed doctype');
      i = end + 1;
    } else if (xml.startsWith('</', i)) {
      i += 2;
      const name = readName();
      skipSpace();
      if (xml[i] !== '>') fail('a malformed end tag');
      i++;
      const open = stack.pop();
      if (!open || open.name !== name) fail(`</${name}> closing <${open?.name ?? 'nothing'}>`);
    } else {
      i++;
      const parent = stack[stack.length - 1];
      const el: XElement = { kind: 'element', name: readName(), attrs: [], children: [] };
      if (parent) parent.children.push(el);
      else if (root) fail('a second root element');
      else root = el;
      for (;;) {
        skipSpace();
        if (xml.startsWith('/>', i)) {
          i += 2;
          break;
        }
        if (xml[i] === '>') {
          i++;
          stack.push(el);
          break;
        }
        const attr = readName();
        skipSpace();
        if (xml[i] !== '=') fail(`an attribute ${attr} with no value`);
        i++;
        skipSpace();
        const quote = xml[i];
        if (quote !== '"' && quote !== "'") fail(`an unquoted value for ${attr}`);
        const end = xml.indexOf(quote, i + 1);
        if (end < 0) fail(`an unclosed value for ${attr}`);
        el.attrs.push([attr, decodeEntities(xml.slice(i + 1, end))]);
        i = end + 1;
      }
    }
  }
  if (stack.length > 0) fail(`<${stack[stack.length - 1].name}> never closed`);
  if (!root) fail('no root element');
  return { root: root! };
}

/** The part of a qualified name after its prefix: `epub:type` is `type`. */
export function localName(name: string): string {
  const colon = name.indexOf(':');
  return colon < 0 ? name : name.slice(colon + 1);
}

export function getAttr(el: XElement, name: string): string | undefined {
  return el.attrs.find(([k]) => k === name)?.[1];
}

export function setAttr(el: XElement, name: string, value: string): void {
  const found = el.attrs.find(([k]) => k === name);
  if (found) found[1] = value;
  else el.attrs.push([name, value]);
}

export function removeAttr(el: XElement, name: string): void {
  el.attrs = el.attrs.filter(([k]) => k !== name);
}

export function childElements(el: XElement): XElement[] {
  return el.children.filter((c): c is XElement => c.kind === 'element');
}

/** Every element under `el`, in document order, `el` itself left out. */
export function* descendants(el: XElement): Generator<XElement> {
  for (const c of el.children) {
    if (c.kind === 'element') {
      yield c;
      yield* descendants(c);
    }
  }
}

/** The elements under `el` (itself included) whose local name is `name`. */
export function findAll(el: XElement, name: string): XElement[] {
  const out: XElement[] = [];
  if (localName(el.name) === name) out.push(el);
  for (const d of descendants(el)) if (localName(d.name) === name) out.push(d);
  return out;
}

/** The text content of `el`, all descendants' text joined. */
export function textContent(el: XElement): string {
  return el.children.map((c) => (c.kind === 'text' ? c.text : textContent(c))).join('');
}

export function escapeText(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

export function escapeAttr(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/\n/g, '&#10;')
    .replace(/\t/g, '&#9;')
    .replace(/\r/g, '&#13;');
}

/** How an empty element is written. `closeEmpty` names the elements written
 * `<p></p>` rather than `<p/>`: KF8 wants that for every element that can
 * hold text (skeleton.py's close_self_closing_tags). */
export interface SerializeOptions {
  closeEmpty?: ReadonlySet<string>;
}

export function openTag(el: XElement): string {
  return `<${el.name}${el.attrs.map(([k, v]) => ` ${k}="${escapeAttr(v)}"`).join('')}`;
}

export function serialize(node: XNode, opts: SerializeOptions = {}): string {
  if (node.kind === 'text') return escapeText(node.text);
  const open = openTag(node);
  if (node.children.length === 0) {
    return opts.closeEmpty?.has(node.name) ? `${open}></${node.name}>` : `${open}/>`;
  }
  return `${open}>${node.children.map((c) => serialize(c, opts)).join('')}</${node.name}>`;
}
