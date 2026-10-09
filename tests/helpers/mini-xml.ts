// A tiny, strict XML reader for tests: enough to prove a generated FDX is
// well-formed and to walk it as a tree. It throws on anything malformed it
// can see (unbalanced or crossed tags, a bad attribute, an unknown entity, a
// bare & or <, a character XML 1.0 forbids, text outside the root) rather
// than guessing. No DTDs, CDATA or processing instructions beyond the XML
// declaration: the FDX writer emits none, so meeting one here is a failure.

export interface XmlElement {
  name: string;
  attrs: Record<string, string>;
  children: XmlNode[];
}
export type XmlNode = XmlElement | string;

const NAME = /^[A-Za-z_][\w.-]*/;
// XML 1.0 Char production, minus nothing: tab, LF, CR, and the legal ranges.
const ILLEGAL_CHAR = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\uFFFE\uFFFF]|[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/;

function decode(raw: string, where: string): string {
  if (/[<]/.test(raw)) throw new Error(`a bare < in ${where}`);
  return raw.replace(/&([^;]*);?/g, (whole, ref: string) => {
    if (!whole.endsWith(';')) throw new Error(`a bare & in ${where}`);
    switch (ref) {
      case 'amp': return '&';
      case 'lt': return '<';
      case 'gt': return '>';
      case 'quot': return '"';
      case 'apos': return "'";
    }
    const num = /^#(?:x([0-9a-fA-F]+)|([0-9]+))$/.exec(ref);
    if (!num) throw new Error(`an unknown entity &${ref}; in ${where}`);
    const cp = num[1] !== undefined ? parseInt(num[1], 16) : parseInt(num[2], 10);
    const ch = String.fromCodePoint(cp);
    if (ILLEGAL_CHAR.test(ch)) throw new Error(`a reference to a forbidden character in ${where}`);
    return ch;
  });
}

/** Parse `xml` into its root element, or throw saying what is wrong. */
export function parseXml(xml: string): XmlElement {
  if (ILLEGAL_CHAR.test(xml)) throw new Error('the document holds a character XML 1.0 forbids');
  let i = 0;
  const decl = /^<\?xml\s[^?]*\?>/.exec(xml);
  if (decl) i = decl[0].length;

  const stack: XmlElement[] = [];
  let root: XmlElement | null = null;

  while (i < xml.length) {
    const lt = xml.indexOf('<', i);
    const textEnd = lt === -1 ? xml.length : lt;
    if (textEnd > i) {
      const raw = xml.slice(i, textEnd);
      const text = decode(raw, 'text');
      if (stack.length === 0) {
        if (text.trim() !== '') throw new Error('text outside the root element');
      } else {
        stack[stack.length - 1].children.push(text);
      }
      i = textEnd;
      continue;
    }
    // At a '<'.
    if (xml.startsWith('<!--', i)) {
      const end = xml.indexOf('-->', i + 4);
      if (end === -1) throw new Error('an unterminated comment');
      i = end + 3;
      continue;
    }
    if (xml.startsWith('</', i)) {
      const m = /^<\/([A-Za-z_][\w.-]*)\s*>/.exec(xml.slice(i));
      if (!m) throw new Error(`a malformed closing tag at ${i}`);
      const open = stack.pop();
      if (!open || open.name !== m[1]) {
        throw new Error(`</${m[1]}> closes ${open ? `<${open.name}>` : 'nothing'}`);
      }
      i += m[0].length;
      continue;
    }
    // An opening or empty tag.
    let j = i + 1;
    const nm = NAME.exec(xml.slice(j));
    if (!nm) throw new Error(`a malformed tag at ${i}`);
    const el: XmlElement = { name: nm[0], attrs: {}, children: [] };
    j += nm[0].length;
    for (;;) {
      const ws = /^\s*/.exec(xml.slice(j))![0];
      j += ws.length;
      if (xml.startsWith('/>', j)) { j += 2; break; }
      if (xml[j] === '>') { j += 1; break; }
      if (ws.length === 0) throw new Error(`no space before an attribute of <${el.name}>`);
      const am = /^([A-Za-z_][\w.:-]*)\s*=\s*"([^"]*)"/.exec(xml.slice(j));
      if (!am) throw new Error(`a malformed attribute on <${el.name}>`);
      if (am[1] in el.attrs) throw new Error(`a repeated attribute ${am[1]} on <${el.name}>`);
      el.attrs[am[1]] = decode(am[2], `attribute ${am[1]}`);
      j += am[0].length;
    }
    const selfClosing = xml[j - 2] === '/';
    if (stack.length === 0) {
      if (root) throw new Error('a second root element');
      root = el;
    } else {
      stack[stack.length - 1].children.push(el);
    }
    if (!selfClosing) stack.push(el);
    i = j;
  }
  if (stack.length > 0) throw new Error(`<${stack[stack.length - 1].name}> is never closed`);
  if (!root) throw new Error('no root element');
  return root;
}

export function childElements(el: XmlElement, name?: string): XmlElement[] {
  return el.children.filter(
    (c): c is XmlElement => typeof c !== 'string' && (name === undefined || c.name === name),
  );
}

export function textOf(el: XmlElement): string {
  return el.children.map((c) => (typeof c === 'string' ? c : textOf(c))).join('');
}
