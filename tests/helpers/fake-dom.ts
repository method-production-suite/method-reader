// A small DOM and a stand-in Tauri, enough to mount the window's real
// modules (desktop/ui/*.js) and drive them: create, append, select, click,
// type, focus. No layout and no rendering: a structural stand-in, not a
// browser. What needs a browser lives in desktop-layout-measured.test.ts.
//
// Shared because tests/desktop-ui.test.ts used to grow a fresh fifty-line
// Node class in every describe that mounted something, and the ones that did
// not bother read the module's source text instead.

type Handler = (event: FakeEvent) => unknown;

export interface FakeEvent {
  type: string;
  target: FakeNode | null;
  currentTarget: FakeNode | null;
  key?: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  payload?: unknown;
  defaultPrevented: boolean;
  preventDefault(): void;
  [extra: string]: unknown;
}

export function makeEvent(type: string, extra: Record<string, unknown> = {}): FakeEvent {
  const event: FakeEvent = {
    type,
    target: null,
    currentTarget: null,
    defaultPrevented: false,
    preventDefault() { event.defaultPrevented = true; },
    ...extra,
  };
  return event;
}

// ------------------------------------------------------------- selectors

interface Compound {
  tag: string | null;
  id: string | null;
  classes: string[];
  attrs: { name: string; value: string | null }[];
  nots: Compound[];
}

function parseCompound(text: string): Compound {
  const out: Compound = { tag: null, id: null, classes: [], attrs: [], nots: [] };
  let rest = text.trim();
  const tag = /^[a-zA-Z][\w-]*|^\*/.exec(rest);
  if (tag) {
    if (tag[0] !== '*') out.tag = tag[0].toLowerCase();
    rest = rest.slice(tag[0].length);
  }
  while (rest.length > 0) {
    let m: RegExpExecArray | null;
    if ((m = /^#([\w-]+)/.exec(rest))) out.id = m[1];
    else if ((m = /^\.([\w-]+)/.exec(rest))) out.classes.push(m[1]);
    else if ((m = /^\[([\w-]+)(?:="([^"]*)")?\]/.exec(rest))) out.attrs.push({ name: m[1], value: m[2] ?? null });
    else if ((m = /^:not\((\[[^\]]*\]|[^)]*)\)/.exec(rest))) out.nots.push(parseCompound(m[1]));
    else throw new Error(`fake-dom: unsupported selector part "${rest}" in "${text}"`);
    rest = rest.slice(m[0].length);
  }
  return out;
}

function matchesCompound(node: FakeNode, c: Compound): boolean {
  if (node.nodeType !== 1) return false;
  if (c.tag !== null && node.localName !== c.tag) return false;
  if (c.id !== null && node.id !== c.id) return false;
  for (const cls of c.classes) if (!node.classList.contains(cls)) return false;
  for (const { name, value } of c.attrs) {
    const actual = node.getAttribute(name);
    if (actual === null) return false;
    if (value !== null && actual !== value) return false;
  }
  for (const not of c.nots) if (matchesCompound(node, not)) return false;
  return true;
}

/** One selector, no commas: compounds joined by descendant spaces. */
function matchesChain(node: FakeNode, chain: Compound[]): boolean {
  if (!matchesCompound(node, chain[chain.length - 1])) return false;
  let at: FakeNode | null = node.parentNode;
  for (let i = chain.length - 2; i >= 0; i -= 1) {
    while (at !== null && !matchesCompound(at, chain[i])) at = at.parentNode;
    if (at === null) return false;
    at = at.parentNode;
  }
  return true;
}

function parseSelector(selector: string): Compound[][] {
  return selector.split(',').map((one) => one.trim().split(/\s+/).map(parseCompound));
}

// ------------------------------------------------------------- nodes

export class FakeNode {
  nodeType: number;
  localName: string;
  nodeValue: string | null = null;
  parentNode: FakeNode | null = null;
  childNodes: FakeNode[] = [];
  attributes = new Map<string, string>();
  dataset: Record<string, string> = {};
  handlers = new Map<string, Handler[]>();
  hidden = false;
  disabled = false;
  checked = false;
  value = '';
  open = false;
  innerHTMLSet: string | null = null;
  /** Layout numbers a test may set; nothing computes them. */
  offsetTop = 0;
  offsetHeight = 0;
  offsetWidth = 0;
  clientHeight = 0;
  scrollTop = 0;
  scrollHeight = 0;
  /** Every focus() this node received, for tests that care who moved it. */
  focusCount = 0;
  ownerDocument: FakeDocument | null;
  [extra: string]: unknown;

  constructor(tag: string, doc: FakeDocument | null, nodeType = 1) {
    this.nodeType = nodeType;
    this.localName = tag.toLowerCase();
    this.ownerDocument = doc;
  }

  get tagName(): string { return this.localName.toUpperCase(); }
  get parentElement(): FakeNode | null { return this.parentNode; }
  get children(): FakeNode[] { return this.childNodes.filter((n) => n.nodeType === 1); }
  get childElementCount(): number { return this.children.length; }
  get firstChild(): FakeNode | null { return this.childNodes[0] ?? null; }
  get lastChild(): FakeNode | null { return this.childNodes[this.childNodes.length - 1] ?? null; }
  get nextSibling(): FakeNode | null {
    const p = this.parentNode;
    return p === null ? null : p.childNodes[p.childNodes.indexOf(this) + 1] ?? null;
  }

  get id(): string { return this.attributes.get('id') ?? ''; }
  set id(value: string) { this.attributes.set('id', String(value)); }
  get className(): string { return this.attributes.get('class') ?? ''; }
  set className(value: string) { this.attributes.set('class', String(value)); }
  get tabIndex(): number {
    const raw = this.attributes.get('tabindex');
    return raw === undefined ? -1 : Number(raw);
  }
  set tabIndex(value: number) { this.attributes.set('tabindex', String(value)); }

  get classList() {
    const self = this;
    const parts = () => self.className.split(/\s+/).filter(Boolean);
    return {
      contains: (cls: string) => parts().includes(cls),
      add: (...names: string[]) => {
        self.className = [...parts(), ...names.filter((n) => !parts().includes(n))].join(' ');
      },
      remove: (...names: string[]) => { self.className = parts().filter((c) => !names.includes(c)).join(' '); },
      toggle: (cls: string, force?: boolean) => {
        const want = force === undefined ? !parts().includes(cls) : force;
        self.className = parts().filter((c) => c !== cls).concat(want ? [cls] : []).join(' ');
        return want;
      },
    };
  }

  setAttribute(name: string, value: unknown) {
    const v = String(value);
    if (name.startsWith('data-')) {
      const key = name.slice(5).replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());
      this.dataset[key] = v;
    }
    if (name === 'hidden') this.hidden = true;
    else if (name === 'disabled') this.disabled = true;
    this.attributes.set(name, v);
  }
  getAttribute(name: string): string | null {
    if (name === 'hidden') return this.hidden ? '' : null;
    if (name === 'disabled') return this.disabled ? '' : null;
    if (name.startsWith('data-')) {
      const key = name.slice(5).replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());
      if (key in this.dataset) return this.dataset[key];
    }
    return this.attributes.get(name) ?? null;
  }
  hasAttribute(name: string) { return this.getAttribute(name) !== null; }
  removeAttribute(name: string) {
    if (name === 'hidden') this.hidden = false;
    if (name === 'disabled') this.disabled = false;
    this.attributes.delete(name);
  }

  set innerHTML(value: string) { this.innerHTMLSet = value; }
  get innerHTML(): string { return this.innerHTMLSet ?? ''; }

  get textContent(): string {
    if (this.nodeType === 3) return this.nodeValue ?? '';
    return this.childNodes.map((k) => k.textContent).join('');
  }
  set textContent(value: string) {
    for (const k of this.childNodes) k.parentNode = null;
    this.childNodes = [];
    const s = value === null || value === undefined ? '' : String(value);
    if (s !== '') this.append(s);
  }

  private adopt(n: FakeNode | string): FakeNode {
    if (typeof n === 'string') {
      const t = new FakeNode('#text', this.ownerDocument, 3);
      t.nodeValue = n;
      return t;
    }
    if (n.parentNode !== null) n.parentNode.removeChild(n);
    n.parentNode = this;
    return n;
  }

  /** Real Node.append() prints a null as the word "null"; so does this, so
   *  a test can catch a surface that hands one over. */
  append(...nodes: Array<FakeNode | string>) {
    for (const n of nodes) {
      const node = this.adopt(n === null || n === undefined ? String(n) : n);
      node.parentNode = this;
      this.childNodes.push(node);
    }
  }
  appendChild(node: FakeNode) { this.append(node); return node; }
  prepend(...nodes: Array<FakeNode | string>) {
    const adopted = nodes.map((n) => this.adopt(n));
    for (const n of adopted) n.parentNode = this;
    this.childNodes.unshift(...adopted);
  }
  insertBefore(node: FakeNode, ref: FakeNode | null) {
    const adopted = this.adopt(node);
    const at = ref === null ? -1 : this.childNodes.indexOf(ref);
    if (at < 0) this.childNodes.push(adopted);
    else this.childNodes.splice(at, 0, adopted);
    return adopted;
  }
  removeChild(child: FakeNode) {
    const i = this.childNodes.indexOf(child);
    if (i >= 0) this.childNodes.splice(i, 1);
    child.parentNode = null;
    return child;
  }
  remove() { this.parentNode?.removeChild(this); }
  replaceWith(...nodes: FakeNode[]) {
    const p = this.parentNode;
    if (p === null) return;
    const at = p.childNodes.indexOf(this);
    p.removeChild(this);
    const adopted = nodes.map((n) => p.adopt(n));
    p.childNodes.splice(at, 0, ...adopted);
  }
  replaceChildren(...nodes: Array<FakeNode | string>) {
    this.textContent = '';
    this.append(...nodes);
  }
  contains(node: unknown): boolean {
    for (let at = node as FakeNode | null; at; at = at.parentNode) if (at === this) return true;
    return false;
  }

  get isConnected(): boolean {
    let at: FakeNode | null = this;
    while (at.parentNode !== null) at = at.parentNode;
    return at.nodeType === 9;
  }

  // ---- events

  addEventListener(type: string, fn: Handler) {
    this.handlers.set(type, [...(this.handlers.get(type) ?? []), fn]);
  }
  removeEventListener(type: string, fn: Handler) {
    this.handlers.set(type, (this.handlers.get(type) ?? []).filter((h) => h !== fn));
  }
  /** Bubbles to the root, as click, input, change and keydown do. Returns
   *  the first handler's result so a test can await an async click. */
  dispatchEvent(event: FakeEvent): unknown {
    event.target ??= this;
    let first: unknown;
    let seen = false;
    for (let at: FakeNode | null = this; at; at = at.parentNode) {
      event.currentTarget = at;
      for (const fn of [...(at.handlers.get(event.type) ?? [])]) {
        const result = fn(event);
        if (!seen) { first = result; seen = true; }
      }
    }
    return first;
  }
  fire(type: string, extra: Record<string, unknown> = {}): unknown {
    return this.dispatchEvent(makeEvent(type, extra));
  }
  click(): unknown {
    if (this.disabled) return undefined;
    return this.fire('click');
  }
  /** Sets a value the way a reader typing or dragging would, then fires the
   *  event the surface listens for. */
  input(value: string, type = 'input'): unknown {
    this.value = value;
    return this.fire(type);
  }
  focus() {
    this.focusCount += 1;
    if (this.ownerDocument) this.ownerDocument.activeElement = this;
  }
  blur() {
    if (this.ownerDocument?.activeElement === this) this.ownerDocument.activeElement = null;
  }

  // ---- dialog

  showModal() { this.open = true; }
  show() { this.open = true; }
  close() { this.open = false; this.fire('close'); }

  // ---- layout no-ops

  scrollIntoView(options?: unknown) { this.scrolledIntoView = options ?? true; }
  getBoundingClientRect() { return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; }

  // ---- selection

  matches(selector: string): boolean {
    return parseSelector(selector).some((chain) => matchesChain(this, chain));
  }
  closest(selector: string): FakeNode | null {
    const chains = parseSelector(selector);
    for (let at: FakeNode | null = this; at; at = at.parentNode) {
      if (chains.some((chain) => matchesChain(at!, chain))) return at;
    }
    return null;
  }
  /** Every element below this one, in document order. */
  descendants(): FakeNode[] {
    const out: FakeNode[] = [];
    for (const k of this.childNodes) {
      if (k.nodeType === 1) out.push(k, ...k.descendants());
    }
    return out;
  }
  querySelectorAll(selector: string): FakeNode[] {
    const chains = parseSelector(selector);
    return this.descendants().filter((n) => chains.some((chain) => matchesChain(n, chain)));
  }
  querySelector(selector: string): FakeNode | null {
    return this.querySelectorAll(selector)[0] ?? null;
  }
  getElementById(id: string): FakeNode | null {
    return this.descendants().find((n) => n.id === id) ?? null;
  }

  // ---- test conveniences (not DOM)

  /** Every button below this one whose text is exactly `label`. */
  buttons(label?: string): FakeNode[] {
    return this.querySelectorAll('button').filter((b) => label === undefined || b.textContent.trim() === label);
  }
  button(label: string): FakeNode {
    const found = this.buttons(label);
    if (found.length !== 1) {
      const all = this.buttons().map((b) => JSON.stringify(b.textContent)).join(', ');
      throw new Error(`fake-dom: ${found.length} buttons labelled "${label}" (there are: ${all})`);
    }
    return found[0];
  }
  /** The text a sighted reader would see: hidden subtrees left out. */
  get visibleText(): string {
    if (this.nodeType === 3) return this.nodeValue ?? '';
    if (this.hidden) return '';
    return this.childNodes.map((k) => k.visibleText).join('');
  }
}

export class FakeDocument extends FakeNode {
  activeElement: FakeNode | null = null;
  adoptedStyleSheets: unknown[] = [];
  documentElement: FakeNode;
  body: FakeNode;

  constructor() {
    super('#document', null, 9);
    this.ownerDocument = this;
    this.documentElement = this.createElement('html');
    this.body = this.createElement('body');
    this.documentElement.append(this.body);
    this.append(this.documentElement);
  }

  createElement(tag: string): FakeNode { return new FakeNode(tag, this); }
  createElementNS(_ns: string, tag: string): FakeNode { return new FakeNode(tag, this); }
  createTextNode(value: string): FakeNode {
    const t = new FakeNode('#text', this, 3);
    t.nodeValue = String(value);
    return t;
  }
  /** A copy of a node from any fake document, owned by this one. */
  importNode(node: FakeNode, deep = false): FakeNode {
    const copy = new FakeNode(node.localName, this, node.nodeType);
    copy.nodeValue = node.nodeValue;
    for (const [name, value] of node.attributes) copy.setAttribute(name, value);
    if (deep) for (const kid of node.childNodes) copy.append(this.importNode(kid, true));
    return copy;
  }
}

/** Every class name used anywhere in this subtree. */
export function classesIn(root: FakeNode): Set<string> {
  const out = new Set<string>();
  for (const n of [root, ...root.descendants()]) for (const c of n.className.split(/\s+/)) if (c) out.add(c);
  return out;
}

// ------------------------------------------------------------- globals

/** Installs globals for the duration of one test and puts back whatever was
 *  there before, absent ones included. */
export function withGlobals() {
  const g = globalThis as unknown as Record<string, unknown>;
  const saved = new Map<string, { had: boolean; value: unknown }>();
  return {
    set(name: string, value: unknown) {
      if (!saved.has(name)) saved.set(name, { had: name in g, value: g[name] });
      g[name] = value;
    },
    restore() {
      for (const [name, { had, value }] of saved) {
        if (had) g[name] = value;
        else delete g[name];
      }
      saved.clear();
    },
  };
}

/** A storage that behaves like localStorage, strings and all. */
export function fakeStorage(seed: Record<string, string> = {}) {
  const map = new Map(Object.entries(seed));
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: unknown) => { map.set(k, String(v)); },
    removeItem: (k: string) => { map.delete(k); },
    dump: () => Object.fromEntries(map),
  };
}

// ------------------------------------------------------------- Tauri

export interface EngineCall {
  args: string[];
  settled: boolean;
  answer(value: unknown): void;
  fail(message: string): void;
}

export const HOLD = Symbol('hold');

/** window.__TAURI__, with the engine answered by `respond` (or held for the
 *  test to answer when it returns HOLD), and the dialogs, the opener and the
 *  event bus recorded. */
export function fakeTauri(respond: (args: string[]) => unknown = () => HOLD) {
  const calls: EngineCall[] = [];
  const listeners = new Map<string, Array<(event: { payload: unknown }) => void>>();
  const dialogs: { kind: string; options: unknown; resolve: (v: unknown) => void }[] = [];
  const opened: string[] = [];
  let respondWith = respond;

  const ask = (kind: string, options: unknown) =>
    new Promise((resolve) => { dialogs.push({ kind, options, resolve }); });

  const tauri = {
    core: {
      invoke: (cmd: string, payload: { args: string[] }) => {
        if (cmd === 'pick_file') return ask('pick_file', null);
        if (cmd !== 'run_engine') return Promise.reject(new Error(`no such command ${cmd}`));
        return new Promise<string>((resolve, reject) => {
          const call: EngineCall = {
            args: payload.args,
            settled: false,
            answer(value) {
              if (call.settled) return;
              call.settled = true;
              resolve(typeof value === 'string' ? value : JSON.stringify(value));
            },
            fail(message) {
              if (call.settled) return;
              call.settled = true;
              reject(message);
            },
          };
          calls.push(call);
          const reply = respondWith(payload.args);
          if (reply !== HOLD) call.answer(reply);
        });
      },
    },
    event: {
      listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
        listeners.set(name, [...(listeners.get(name) ?? []), handler]);
        return () => listeners.set(name, (listeners.get(name) ?? []).filter((h) => h !== handler));
      },
    },
    dialog: {
      open: (options: unknown) => ask('open', options),
      save: (options: unknown) => ask('save', options),
    },
    opener: { openUrl: async (url: string) => { opened.push(url); } },
  };

  return {
    tauri,
    calls,
    dialogs,
    opened,
    /** The calls whose first argument is `verb` (a PDF path for a convert). */
    callsTo(verb: string) { return calls.filter((c) => c.args[0] === verb); },
    pending() { return calls.filter((c) => !c.settled); },
    respond(fn: (args: string[]) => unknown) { respondWith = fn; },
    emit(name: string, payload: unknown) {
      for (const h of listeners.get(name) ?? []) h({ payload });
    },
    listening(name: string) { return (listeners.get(name) ?? []).length; },
    /** Answers every call still held with a refusal and cancels every open
     *  dialog, so nothing a test left behind counts toward app.js's busy
     *  count in the next one. */
    drain() {
      for (const c of calls) c.answer({ ok: false, error: { code: 'internal', message: 'drained' } });
      for (const d of dialogs.splice(0)) d.resolve(null);
    },
  };
}

export type FakeTauri = ReturnType<typeof fakeTauri>;

/** Lets every promise already settled run its callbacks, then one timer. */
export const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
export async function settle(times = 3) { for (let i = 0; i < times; i += 1) await tick(); }

// ------------------------------------------------------------- a whole page

/** Answers the two things read.js's splitPreview asks of a parsed preview:
 *  its <style>, which can be removed, and the document's outer HTML. Regexes,
 *  not a parser: it proves nothing about HTML, only that the window takes
 *  the stylesheet out and passes the rest on. */
export class FakeDOMParser {
  parseFromString(html: string) {
    let css: string | null = (String(html).match(/<style>([\s\S]*?)<\/style>/) ?? [])[1] ?? null;
    return {
      querySelector: (selector: string) => (selector === 'style' && css !== null
        ? { textContent: css, remove: () => { css = null; } }
        : null),
      get documentElement() {
        const body = String(html).replace(/<\?xml[^>]*\?>\s*/, '')
          .replace(/<style>[\s\S]*?<\/style>/, css === null ? '' : `<style>${css}</style>`);
        return { outerHTML: body };
      },
    };
  }
}

export interface FakePage {
  doc: FakeDocument;
  tauri: FakeTauri;
  storage: ReturnType<typeof fakeStorage>;
  /** Listeners hung on the window itself (focus, resize, keydown). */
  windowListeners: Map<string, Array<(event: unknown) => unknown>>;
  /** Calls every listener of `type` on the window. */
  fireWindow(type: string, extra?: Record<string, unknown>): FakeEvent;
  /** A pane already in the document, for mounting one surface into. */
  pane(): FakeNode;
  /** Answers what is still held, cancels open dialogs, puts globals back. */
  close(): Promise<void>;
}

/** Installs a document, a window with a stand-in Tauri on it, and the few
 *  browser globals the surfaces read (navigator, localStorage, matchMedia,
 *  CSSStyleSheet, getComputedStyle, requestAnimationFrame). */
export function fakePage(options: {
  respond?: (args: string[]) => unknown;
  platform?: string;
  storage?: Record<string, string>;
  tauriExtra?: Record<string, unknown>;
} = {}): FakePage {
  const globals = withGlobals();
  const doc = new FakeDocument();
  const tauri = fakeTauri(options.respond);
  const storage = fakeStorage(options.storage);
  const windowListeners = new Map<string, Array<(event: unknown) => unknown>>();
  const listen = (type: string, fn: (event: unknown) => unknown) => {
    windowListeners.set(type, [...(windowListeners.get(type) ?? []), fn]);
  };
  const unlisten = (type: string, fn: (event: unknown) => unknown) => {
    windowListeners.set(type, (windowListeners.get(type) ?? []).filter((h) => h !== fn));
  };
  globals.set('window', {
    __TAURI__: { ...tauri.tauri, ...(options.tauriExtra ?? {}) },
    addEventListener: listen,
    removeEventListener: unlisten,
  });
  globals.set('addEventListener', listen);
  globals.set('removeEventListener', unlisten);
  globals.set('document', doc);
  globals.set('localStorage', storage);
  globals.set('navigator', { platform: options.platform ?? 'MacIntel', language: 'en-US' });
  globals.set('matchMedia', (media: string) => ({
    media, matches: false, addEventListener() {}, removeEventListener() {},
  }));
  globals.set('CSSStyleSheet', class { text = ''; replaceSync(t: string) { this.text = t; } });
  globals.set('getComputedStyle', () => ({ getPropertyValue: () => '' }));
  globals.set('requestAnimationFrame', () => 0);
  globals.set('DOMParser', FakeDOMParser);
  return {
    doc,
    tauri,
    storage,
    windowListeners,
    fireWindow(type, extra = {}) {
      const event = makeEvent(type, extra);
      event.target = doc.body;
      for (const fn of windowListeners.get(type) ?? []) fn(event);
      return event;
    },
    pane() {
      const pane = doc.createElement('section');
      pane.setAttribute('tabindex', '0');
      doc.body.append(pane);
      return pane;
    },
    async close() {
      tauri.drain();
      await settle();
      globals.restore();
    },
  };
}
